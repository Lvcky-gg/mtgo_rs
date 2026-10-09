"""Run the actual publication shell against a local GitHub CLI stand-in."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[2]
PLATFORMS = ["windows-x64", "macos-arm64", "macos-x64", "linux-flatpak"]


class ReleasePublication(unittest.TestCase):
    def test_publisher_downloads_only_platform_artifacts(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        publish = workflow.split("\n  publish:\n", 1)[1]
        downloads = re.findall(
            r"uses: actions/download-artifact@[^\n]+\n\s+with:\n"
            r"\s+name: ([^\n]+)\n\s+path: dist", publish)
        self.assertCountEqual(downloads, [f"release-{name}" for name in PLATFORMS])
        self.assertNotIn("pattern: release-*", publish)

    def test_new_and_existing_releases_upload_only_installers_and_checksums(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        script = textwrap.dedent(workflow.split(
            "      - name: Create or update the release for this commit\n        run: |\n", 1)[1])
        mock = '''#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
with open(os.environ["MOCK_TRACE"], "a") as log:
    log.write(json.dumps(args) + "\\n")
if args[:2] == ["release", "view"]:
    if "--json" in args:
        print("Generated release notes")
        sys.exit(0)
    sys.exit(0 if os.environ["MOCK_EXISTS"] == "1" else 1)
if args[:2] in (["release", "create"], ["release", "upload"]):
    # Match the failure caused by passing dist/campaign to gh release.
    for arg in args[3:]:
        if arg.startswith("dist/") and not pathlib.Path(arg).is_file():
            print("cannot upload a directory", file=sys.stderr)
            sys.exit(1)
    sys.exit(0)
if args[:2] == ["release", "edit"]:
    sys.exit(0)
sys.exit(2)
'''
        for exists in [False, True]:
            with self.subTest(retry=exists), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "bin").mkdir()
                gh = root / "bin/gh"
                gh.write_text(mock)
                gh.chmod(0o755)
                dist = root / "dist"
                dist.mkdir()
                assets = [
                    "mtgo-rs-v1.2.3-windows-x64.exe",
                    "mtgo-rs-v1.2.3-macos-arm64.dmg",
                    "mtgo-rs-v1.2.3-macos-x64.dmg",
                    "mtgo-rs-v1.2.3-linux-x64.flatpak",
                    "mtgo-rs-v1.2.3-macos-arm64.zip",
                    "mtgo-rs-v1.2.3-macos-x64.zip",
                    "SHA256SUMS.txt",
                ]
                for name in assets:
                    (dist / name).write_bytes(b"installer fixture")
                # The failed run contains this evidence artifact alongside the builds.
                (dist / "campaign").mkdir()
                (dist / "campaign/report.json").write_text("{}")
                (dist / "workspace_tests.log").write_text("private build diagnostics")
                trace = root / "trace.jsonl"
                env = dict(os.environ, PATH=str(root / "bin") + os.pathsep + os.environ["PATH"],
                           RELEASE_VERSION="1.2.3", RELEASE_TAG="v1.2.3", RELEASE_SHA="a" * 40,
                           MOCK_TRACE=str(trace), MOCK_EXISTS=str(int(exists)))
                result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script],
                                        cwd=root, env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                calls = [json.loads(line) for line in trace.read_text().splitlines()]
                uploads = [call for call in calls if call[:2] in
                           (["release", "create"], ["release", "upload"])]
                self.assertEqual(len(uploads), 1)
                self.assertEqual(uploads[0][1], "upload" if exists else "create")
                self.assertEqual([arg for arg in uploads[0] if arg.startswith("dist/")],
                                 ["dist/" + name for name in assets])
                self.assertEqual(calls[-1][:3], ["release", "edit", "v1.2.3"])
                self.assertIn("--draft=false", calls[-1])
                if exists:
                    self.assertIn("--clobber", uploads[0])
                else:
                    self.assertIn("--draft", uploads[0])
                    self.assertIn("--latest", calls[-1])
                    self.assertIn("Report a bug", (root / "release-notes.md").read_text())


if __name__ == "__main__":
    unittest.main()
