import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("release", ROOT / "scripts/release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def test_every_push_bumps_and_breaking_changes_take_precedence(self):
        for messages, expected in [
            (["some things"], "0.1.1"),
            (["docs: hosting", "fix: tutor"], "0.1.1"),
            (["feat(network): hosting"], "0.2.0"),
            (["feat: hosting", "fix!: change protocol"], "1.0.0"),
            (["fix: wire\n\nBREAKING CHANGE: incompatible protocol"], "1.0.0"),
            (["fix: wire\n\nBREAKING-CHANGE: incompatible protocol"], "1.0.0"),
        ]:
            with self.subTest(messages=messages):
                self.assertEqual(release.bump("0.1.0", messages), expected)

    def test_stamping_preserves_locked_dependencies_and_versions_all_workspace_crates(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copy(ROOT / "Cargo.toml", root)
            shutil.copy(ROOT / "Cargo.lock", root)
            for manifest in (ROOT / "crates").glob("*/Cargo.toml"):
                destination = root / manifest.relative_to(ROOT)
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(manifest, destination)
            metadata = Path("packaging/flatpak/io.github.lvcky_gg.MtgoRs.metainfo.xml")
            (root / metadata).parent.mkdir(parents=True)
            shutil.copy(ROOT / metadata, root / metadata)
            before = tomllib.loads((root / "Cargo.lock").read_text())["package"]
            release.stamp(root, "1.2.3")
            after = tomllib.loads((root / "Cargo.lock").read_text())["package"]
            for old, new in zip(before, after, strict=True):
                if "source" in old:
                    self.assertEqual(old, new)
                else:
                    self.assertEqual(new["version"], "1.2.3")
            self.assertEqual(tomllib.loads((root / "Cargo.toml").read_text())
                             ["workspace"]["package"]["version"], "1.2.3")
            self.assertEqual(ET.parse(root / metadata).find("releases/release")
                             .attrib["version"], "1.2.3")

    def test_invalid_versions_are_rejected(self):
        for value in ["01.2.3", "1.2", "1.2.3-rc.1", "1.2.3;bad"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                release.version_tuple(value)

    def test_flatpak_builds_offline_with_required_game_permissions(self):
        manifest = json.loads((ROOT / "packaging/flatpak/io.github.lvcky_gg.MtgoRs.json").read_text())
        self.assertIn("--share=network", manifest["finish-args"])
        self.assertIn("--socket=pulseaudio", manifest["finish-args"])
        self.assertFalse(any(value.startswith("--filesystem=") for value in manifest["finish-args"]))
        self.assertIn("--frozen", manifest["modules"][0]["build-commands"][0])

    @unittest.skipUnless(shutil.which("flatpak"), "Flatpak is not installed")
    def test_flatpak_export_branch_can_be_bundled_by_the_release_workflow(self):
        manifest = json.loads((ROOT / "packaging/flatpak/io.github.lvcky_gg.MtgoRs.json").read_text())
        # The bundle command names stable. Without a declared branch the builder
        # exports master, and build-bundle fails with "Refspec .../stable not found".
        self.assertEqual(manifest.get("default-branch"), "stable")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app = root / "app"
            (app / "files/bin").mkdir(parents=True)
            (app / "export").mkdir()
            arch = subprocess.check_output(["flatpak", "--default-arch"], text=True).strip()
            (app / "metadata").write_text(
                f'[Application]\nname={manifest["app-id"]}\n'
                f'runtime={manifest["runtime"]}/{arch}/{manifest["runtime-version"]}\n'
                f'command={manifest["command"]}\n')
            executable = app / "files/bin" / manifest["command"]
            executable.write_text("#!/bin/sh\nexit 0\n")
            executable.chmod(0o755)
            subprocess.run(["flatpak", "build-export", "--disable-sandbox",
                            str(root / "repo"), str(app), manifest["default-branch"]],
                           check=True, capture_output=True)
            bundle = root / "release.flatpak"
            subprocess.run(["flatpak", "build-bundle", str(root / "repo"),
                            str(bundle), manifest["app-id"], "stable"],
                           check=True, capture_output=True)
            self.assertGreater(bundle.stat().st_size, 0)

    def test_version_is_numeric_and_retries_reuse_the_commit_tag(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copy(ROOT / "scripts/release.py", root)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.1.0"\n')
            def git(*args):
                return subprocess.check_output(["git", *args], cwd=root, text=True).strip()
            git("init", "-q")
            git("config", "user.email", "release-test@example.invalid")
            git("config", "user.name", "Release test")
            git("add", ".")
            git("commit", "-qm", "initial")
            before = git("rev-parse", "HEAD")
            git("tag", "v0.1.9")
            git("tag", "v0.1.10")
            git("tag", "v99.0.0-rc.1")
            git("commit", "--allow-empty", "-qm", "feat: release")
            sha = git("rev-parse", "HEAD")
            command = ["python3", "release.py", "version", "--sha", sha, "--before", before]
            self.assertEqual(subprocess.check_output(command, cwd=root, text=True).strip(), "0.2.0")
            git("tag", "v0.2.0")
            git("tag", "v3.0.0", before)
            self.assertEqual(subprocess.check_output(command, cwd=root, text=True).strip(), "0.2.0")


if __name__ == "__main__":
    unittest.main()
