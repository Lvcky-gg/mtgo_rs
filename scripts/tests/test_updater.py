"""Exercise the real installer scripts against disposable apps and SQLite data."""
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / 'crates/mtg-app/src/update'


class InstallerTests(unittest.TestCase):
    @unittest.skipUnless(os.name == 'posix', 'POSIX installer')
    def test_mac_swap_and_failed_swap_preserve_database_and_launch_environment(self):
        for fail in (False, True):
            with self.subTest(fail=fail), tempfile.TemporaryDirectory(prefix="mtgo update ' ") as tmp:
                root = Path(tmp)
                target = root / 'MTGO RS.app'
                stage = root / '.mtgo-update-test'
                stage.mkdir()
                new = stage / 'new.app'
                marker = root / 'launched'
                for app, version in [(target, 'old'), (new, 'new')]:
                    executable = app / 'Contents/MacOS/mtg-gui'
                    executable.parent.mkdir(parents=True)
                    executable.write_text(f'#!/bin/sh\nprintf "%s\\n" "{version}" "$PWD" "$MTGO_RS_DB" "$@" > "$UPDATE_MARKER"\n')
                    executable.chmod(0o755)
                db = root / 'cards.sqlite'
                connection = sqlite3.connect(db)
                connection.execute('pragma journal_mode=wal')
                connection.execute('create table decks (name text)')
                connection.execute("insert into decks values ('My existing deck')")
                connection.commit()
                files = [db, Path(str(db) + '-wal'), Path(str(db) + '-shm')]
                before = {f: f.read_bytes() for f in files}
                env = dict(os.environ, MTGO_RS_DB='cards.sqlite', UPDATE_MARKER=str(marker))
                subprocess.run(['sh', str(SCRIPTS / 'macos.sh'), str(target),
                                str(stage / 'missing.app' if fail else new), str(stage),
                                '2000000000', str(root)], env=env, check=True, capture_output=True)
                for _ in range(100):
                    if marker.exists() and len(marker.read_text().splitlines()) >= 4:
                        break
                    time.sleep(.01)
                lines = marker.read_text().splitlines()
                self.assertEqual(lines[:4], ['old' if fail else 'new', str(root), 'cards.sqlite', '--skip-update-once'])
                self.assertEqual(before, {f: f.read_bytes() for f in files})
                self.assertEqual(connection.execute('select name from decks').fetchone()[0], 'My existing deck')
                if fail:
                    self.assertIn('--update-error', lines)
                    self.assertTrue((stage / 'error.txt').exists())
                if not fail:
                    self.assertFalse(stage.exists())
                connection.close()

    @unittest.skipUnless(shutil.which('powershell.exe'), 'Windows PowerShell installer')
    def test_windows_swap_and_rollback_preserve_database(self):
        for fail in (False, True):
            with self.subTest(fail=fail), tempfile.TemporaryDirectory(prefix="mtgo update ' ") as tmp:
                root = Path(tmp)
                target, new, stage = root / 'client.exe', root / 'new.exe', root / '.mtgo-update-test'
                stage.mkdir()
                target.write_bytes(b'old executable')
                if not fail:
                    new.write_bytes(b'new executable')
                db = root / 'cards.sqlite'
                db.write_bytes(b'existing database and decks')
                marker = root / 'launched.json'
                wrapper = root / 'test.ps1'
                # Mock only process launch; execute the shipped file operations unchanged.
                wrapper.write_text('''param($Installer, $Target, $New, $Stage, $WorkingDirectory, $Marker)
function Start-Process {
    param($FilePath, $WorkingDirectory, $ArgumentList)
    @{File=$FilePath; Cwd=$WorkingDirectory; Args=$ArgumentList} | ConvertTo-Json | Set-Content -LiteralPath $Marker -Encoding UTF8
}
& $Installer $Target $New $Stage 2000000000 $WorkingDirectory
''')
                subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                                '-File', str(wrapper), str(SCRIPTS / 'windows.ps1'),
                                str(target), str(new), str(stage), str(root), str(marker)], check=True)
                self.assertEqual(target.read_bytes(), b'old executable' if fail else b'new executable')
                self.assertEqual(db.read_bytes(), b'existing database and decks')
                launched = json.loads(marker.read_text(encoding='utf-8-sig'))
                self.assertEqual(launched['File'], str(target))
                self.assertEqual(launched['Cwd'], str(root))
                self.assertIn('--skip-update-once', launched['Args'])
                self.assertEqual((stage / 'error.txt').exists(), fail)
                if not fail:
                    self.assertFalse(stage.exists())


if __name__ == '__main__':
    unittest.main()
