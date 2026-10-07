"""Guard the distinction between killed mutants and infrastructure failures."""
import importlib.util
from pathlib import Path
import unittest

SOURCE = Path(__file__).resolve().parents[1] / "verify_mutations.py"
SPEC = importlib.util.spec_from_file_location("verify_mutations", SOURCE)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class TestMutationClassification(unittest.TestCase):
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
