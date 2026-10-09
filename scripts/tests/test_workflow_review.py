"""Standard-library guards for verification evidence paths and shell failure propagation."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


class WorkflowEvidenceReview(unittest.TestCase):
    def test_verification_jobs_initialize_before_tee_and_archive_evidence(self):
        for filename, job in [("verification.yml", "verify"), ("release.yml", "test")]:
            with self.subTest(workflow=filename):
                text = (ROOT / ".github/workflows" / filename).read_text()
                self.assertRegex(text, r"defaults:\n  run:\n    shell: bash\n")
                body = text.split(f"\n  {job}:\n", 1)[1]
                body = re.split(r"\n  [a-z_]+:\n", body, maxsplit=1)[0]
                self.assertLess(body.index("mkdir -p verification-artifacts"), body.index("| tee "))
                for log in ("format.log", "clippy.log", "workspace_tests.log"):
                    self.assertIn("tee verification-artifacts/" + log, body)
                self.assertIn("--output verification-artifacts/campaign", body)
                self.assertIn("--campaign verification-artifacts/campaign/report.json", body)
                self.assertRegex(body, r"if: always\(\)\n\s+uses: actions/upload-artifact@")
                self.assertRegex(body, r"path: (?:\|\n\s+)?verification-artifacts/")

    def test_scheduled_structural_fuzz_pipeline_preserves_pipefail(self):
        text = (ROOT / ".github/workflows/verification.yml").read_text()
        body = text.split("- name: Bounded structural fuzz campaign", 1)[1].split("- name:", 1)[0]
        self.assertIn("set -o pipefail", body)
        self.assertIn("| tee verification-artifacts/structural.log", body)

    def test_release_version_checkout_fetch_depth_stays_under_with(self):
        text = (ROOT / ".github/workflows/release.yml").read_text()
        version = text.split("\n  version:\n", 1)[1].split("\n  test:\n", 1)[0]
        self.assertRegex(version, r"with:\n\s+ref:.*\n\s+fetch-depth: 0")
