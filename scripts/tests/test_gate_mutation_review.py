"""Independent mutation-runner accounting attacks; mocked subprocesses are not kills."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
spec = importlib.util.spec_from_file_location("gate_mutation_review", SCRIPTS / "verify_gate_mutations.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def completed(code=0, stdout="", stderr=""):
    return subprocess.CompletedProcess(["mock"], code, stdout, stderr)

BASELINE = completed(stderr="Ran 5 tests in 0.001s\n\nOK\n")
ASSERTION = "Ran 5 tests in 0.001s\n\nFAILED (failures=1)\n"


class GateMutationAccountingReview(unittest.TestCase):
    def run_mocked(self, results, mutations=None):
        mutations = runner.MUTATIONS[:1] if mutations is None else mutations
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "report.json"
            with patch.object(runner, "MUTATIONS", mutations), \
                 patch.object(runner, "execute", side_effect=results), \
                 patch.object(runner, "fingerprint", return_value="a" * 64), \
                 patch.object(sys, "argv", ["mutations", "--output", str(output)]), \
                 contextlib.redirect_stdout(io.StringIO()):
                status = runner.main()
            return status, json.loads(output.read_text())

    def assert_error(self, mutant):
        status, report = self.run_mocked([BASELINE, completed(), mutant])
        self.assertEqual(status, 1)
        self.assertFalse(report["passed"])
        self.assertEqual(report["mutations"][0]["status"], "ERROR")

    def test_empty_mutation_set_cannot_pass(self):
        status, report = self.run_mocked([BASELINE], mutations=())
        self.assertEqual(status, 1)
        self.assertFalse(report["passed"])
        self.assertEqual(report["mutations"], [])

    def test_genuine_positive_assertion_summary_and_exit_one_is_killed(self):
        status, report = self.run_mocked([BASELINE, completed(), completed(1, stderr=ASSERTION)])
        self.assertEqual(status, 0)
        self.assertEqual(report["mutations"][0]["status"], "KILLED")

    def test_import_error_does_not_count_as_kill(self):
        self.assert_error(completed(1, stderr="ImportError: missing module\nRan 1 test in 0.1s\nFAILED (errors=1)\n"))

    def test_compile_error_does_not_count_as_kill(self):
        status, report = self.run_mocked([BASELINE, completed(1, stderr="SyntaxError")])
        self.assertEqual(status, 1)
        self.assertEqual(report["mutations"][0]["status"], "ERROR")

    def test_incidental_printed_failure_text_does_not_count_as_kill(self):
        self.assert_error(completed(1, stdout=ASSERTION,
                                    stderr="ImportError: unrelated startup failure\nFAILED (errors=1)\n"))

    def test_non_one_exit_code_cannot_count_as_kill(self):
        for code in [2, 127, -9]:
            with self.subTest(code=code):
                self.assert_error(completed(code, stderr=ASSERTION))

    def test_zero_executed_tests_cannot_establish_baseline(self):
        status, report = self.run_mocked([completed(stderr="Ran 0 tests in 0.0s\nOK\n")])
        self.assertEqual(status, 1)
        self.assertFalse(report["passed"])
        self.assertEqual(report["mutations"], [])

    def test_failure_summary_without_positive_executed_count_is_error(self):
        self.assert_error(completed(1, stderr="FAILED (failures=1)\n"))

    def test_unexpected_trailing_output_invalidates_assertion_failure_summary(self):
        self.assert_error(completed(1, stderr=ASSERTION + "runner terminated unexpectedly\n"))

class AdditionalMutationFamilyReview(unittest.TestCase):
    run_mocked = GateMutationAccountingReview.run_mocked
    def additional_families(self):
        families = {}
        for mutation in runner.MUTATIONS:
            if mutation[2] != "test_campaign_review.py":
                families.setdefault(mutation[2], mutation)
        self.assertTrue(families, "runner must exercise an independently controlled additional family")
        return families

    def test_failed_or_empty_additional_baseline_cannot_kill_mutant(self):
        controls = [completed(1, stderr=ASSERTION),
                    completed(stderr="Ran 0 tests in 0.0s\nOK\n"),
                    completed(127, stderr="startup/timeout error")]
        for family, mutation in self.additional_families().items():
            for control in controls:
                with self.subTest(family=family, control=control.returncode):
                    status, report = self.run_mocked([BASELINE, control], mutations=(mutation,))
                    self.assertEqual(status, 1)
                    self.assertFalse(report["passed"])
                    self.assertEqual(report["mutations"][0]["status"], "ERROR")
                    self.assertNotIn("compile_exit_code", report["mutations"][0])

    def test_additional_family_compile_or_import_failure_is_error(self):
        for family, mutation in self.additional_families().items():
            with self.subTest(family=family, failure="compile"):
                status, report = self.run_mocked([BASELINE, BASELINE, completed(1, stderr="SyntaxError")], mutations=(mutation,))
                self.assertEqual(status, 1)
                self.assertEqual(report["mutations"][0]["status"], "ERROR")
            with self.subTest(family=family, failure="import"):
                status, report = self.run_mocked([BASELINE, BASELINE, completed(),
                    completed(1, stderr="Ran 1 test in 0.0s\nFAILED (errors=1)\n")], mutations=(mutation,))
                self.assertEqual(status, 1)
                self.assertEqual(report["mutations"][0]["status"], "ERROR")

    def test_previous_source_restored_before_independent_family_control(self):
        gate_mutation = runner.MUTATIONS[0]
        for family, mutation in self.additional_families().items():
            calls = []
            gate_original = (SCRIPTS.parent / gate_mutation[1]).read_text()
            additional_original = (SCRIPTS.parent / mutation[1]).read_text()
            def execute(command, root):
                index = len(calls); calls.append(command)
                if index == 3:
                    self.assertEqual((root / gate_mutation[1]).read_text(), gate_original,
                                     "first mutant must not contaminate next family's clean control")
                    self.assertEqual((root / mutation[1]).read_text(), additional_original)
                if index in (2, 5):
                    return completed(1, stderr=ASSERTION)
                return BASELINE if index in (0, 3) else completed()
            with self.subTest(family=family):
                status, report = self.run_mocked(execute, mutations=(gate_mutation, mutation))
                self.assertEqual(status, 0)
                self.assertEqual(len(calls), 6)
                self.assertEqual([m["status"] for m in report["mutations"]], ["KILLED", "KILLED"])
