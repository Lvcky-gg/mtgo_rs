"""Guard the distinction between killed mutants and infrastructure failures."""
import importlib.util
from pathlib import Path
import unittest
import tempfile

SOURCE = Path(__file__).resolve().parents[1] / "verify_mutations.py"
SPEC = importlib.util.spec_from_file_location("verify_mutations", SOURCE)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class TestMutationClassification(unittest.TestCase):
    def test_targets_cannot_reuse_previous_or_concurrent_mutant_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            environment = {"CARGO_TARGET_DIR": "shared-unsafe-target", "EXAMPLE": "preserved"}
            with MODULE.isolated_target(base, environment) as first:
                first_target = Path(first["CARGO_TARGET_DIR"])
                (first_target / "mutant-artifact").write_text("poisoned")
                with MODULE.isolated_target(base, environment) as second:
                    second_target = Path(second["CARGO_TARGET_DIR"])
                    self.assertNotEqual(first_target, second_target)
                    self.assertFalse((second_target / "mutant-artifact").exists())
                    self.assertEqual(second["EXAMPLE"], "preserved")
                self.assertFalse(second_target.exists())
            self.assertFalse(first_target.exists())
            self.assertEqual(environment["CARGO_TARGET_DIR"], "shared-unsafe-target")

    def test_compile_failure_does_not_kill_mutant(self):
        self.assertEqual(MODULE.test_outcome({"exit_code": 101, "output": "error[E0046]: missing trait item"}), "ERROR")

    def test_zero_tests_do_not_count_as_survival(self):
        self.assertEqual(MODULE.test_outcome({"exit_code": 0, "output": "test result: ok. 0 passed; 0 failed;"}), "ERROR")

    def test_test_failure_is_actual_kill_evidence(self):
        self.assertEqual(MODULE.test_outcome({"exit_code": 101, "output": "test result: FAILED. 0 passed; 1 failed;"}), "FAIL")

    def test_successful_test_is_survival(self):
        self.assertEqual(MODULE.test_outcome({"exit_code": 0, "output": "test result: ok. 1 passed; 0 failed;"}), "PASS")


if __name__ == "__main__":
    unittest.main()
