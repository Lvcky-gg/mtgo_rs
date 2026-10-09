"""Independent bounded-runner tests; no production campaign is executed here."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("campaign_execution_review", ROOT / "scripts/verification_campaign.py")
campaign = importlib.util.module_from_spec(spec)
spec.loader.exec_module(campaign)


class CampaignExecutionReview(unittest.TestCase):
    def test_missing_executable_has_error_record_and_log(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "start.log"
            result = campaign.execute([str(Path(directory) / "missing-executable")], log, dict(os.environ), 1)
            self.assertEqual(result["status"], "ERROR")
            self.assertIsNone(result["exit_code"])
            self.assertTrue(result["error"])
            self.assertTrue(log.read_text())

    def test_timeout_cannot_be_reported_as_success(self):
        with tempfile.TemporaryDirectory() as directory:
            result = campaign.execute([sys.executable, "-c", "import time; time.sleep(10)"],
                                      Path(directory) / "timeout.log", dict(os.environ), 0.05)
            self.assertEqual(result["status"], "TIMEOUT")
            self.assertIsNone(result["exit_code"])
            self.assertLess(result["elapsed_seconds"], 2)

    def orchestrate(self, invalid_template=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for corpus in ("replays", "regressions"):
                folder = root / "tests" / corpus
                folder.mkdir(parents=True)
                (folder / "one.json").write_text("INVALID JSON" if invalid_template else "{}")
            output = root / "evidence"
            def execute(command, log, env, timeout):
                log.write_text("mock execution; not correctness evidence\n")
                return {"command": command, "status": "PASS" if invalid_template else "FAIL",
                        "exit_code": 0 if invalid_template else 1, "log": str(log)}
            with patch.object(campaign, "ROOT", root), patch.object(campaign, "execute", execute), \
                 patch.object(campaign, "fingerprint", return_value="a" * 64), \
                 patch.object(sys, "argv", ["campaign", "--output", str(output)]), \
                 contextlib.redirect_stdout(io.StringIO()):
                status = campaign.main()
            report = json.loads((output / "report.json").read_text())
            self.assertEqual(status, 1)
            self.assertFalse(report["passed"])
            self.assertTrue(report["error"])
            self.assertTrue(report["plan"]["checks"])
            return report

    def test_early_execution_failure_archives_failed_report_and_full_plan(self):
        report = self.orchestrate()
        self.assertEqual(len(report["checks"]), 1)
        self.assertEqual(report["checks"][0]["status"], "FAIL")

    def test_bad_template_json_preserves_failed_report(self):
        report = self.orchestrate(invalid_template=True)
        self.assertGreater(len(report["checks"]), 1)
        self.assertNotIn("semantic_0", [c["name"] for c in report["checks"]])

    def test_nonempty_output_refuses_to_overwrite_existing_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            sentinel = output / "report.json"
            sentinel.write_text("original evidence")
            with patch.object(sys, "argv", ["campaign", "--output", str(output)]), \
                 contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
                campaign.main()
            self.assertEqual(error.exception.code, 2)
            self.assertEqual(sentinel.read_text(), "original evidence")

class CampaignTriageIsolationReview(unittest.TestCase):
    def test_triage_success_or_exception_cannot_promote_original_campaign_failure(self):
        for diagnostic in ["accepted", "exception"]:
            with self.subTest(diagnostic=diagnostic), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                for corpus in ("replays", "regressions"):
                    folder = root / "tests" / corpus; folder.mkdir(parents=True)
                    (folder / "one.json").write_text("{}")
                output = root / "evidence"
                def execute(command, log, env, timeout):
                    failed = len(command) > 1 and command[1] == "replay"
                    log.write_text("mock campaign check\n")
                    return {"command": command, "status": "FAIL" if failed else "PASS",
                            "exit_code": 1 if failed else 0, "log": str(log)}
                diagnosis = {"status": "MINIMIZED", "passed": True, "commands": [],
                             "confidence_promotion": False, "regression_promotion": False}
                with patch.object(campaign, "ROOT", root), patch.object(campaign, "execute", execute), \
                     patch.object(campaign, "fingerprint", return_value="a" * 64), \
                     patch.object(campaign, "triage", side_effect=RuntimeError("mock triage crash") if diagnostic == "exception" else None, return_value=diagnosis), \
                     patch.object(sys, "argv", ["campaign", "--output", str(output)]), \
                     contextlib.redirect_stdout(io.StringIO()):
                    status = campaign.main()
                report = json.loads((output / "report.json").read_text())
                self.assertEqual(status, 1)
                self.assertFalse(report["passed"])
                self.assertIn("corpus fixture failed", report["error"])
                self.assertEqual(report["checks"][-1]["status"], "FAIL")
                self.assertEqual(len(report["checks"]), 3)
                self.assertEqual(len(report["failure_triage"]), 1)
                self.assertEqual(report["failure_triage"][0]["status"], "ERROR" if diagnostic == "exception" else "MINIMIZED")
                self.assertFalse(report["confidence_promotion"])
