"""Independent staging contract; mocked PASS reports are synthetic schema evidence."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/regression_intake.py"


def load_intake():
    spec = importlib.util.spec_from_file_location("regression_intake_review", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class RegressionIntakeReview(unittest.TestCase):
    def exercise(self, attack=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate = root / "candidate.json"
            review_path = root / "review.json"
            output = root / "staging"
            scenario = json.loads((ROOT / "tests/regressions/issue_local_priority_eliminated_player.json").read_text())
            review = {"format_version": 1, "builder": "builder-agent", "reviewer": "independent-rules-agent",
                      "scenario_sha256": "", "expected_behavior": "Priority reaches the surviving player",
                      "evidence": "Independent CR117/800 acceptance review", "kind": "engine"}
            if attack == "same_reviewer": review["reviewer"] = review["builder"]
            if attack == "blank_builder": review["builder"] = " "
            if attack == "blank_reviewer": review["reviewer"] = " "
            if attack == "blank_evidence": review["evidence"] = " "
            if attack == "blank_behavior": review["expected_behavior"] = " "
            if attack == "wrong_kind": review["kind"] = "VERIFIED"
            if attack == "wrong_format": review["format_version"] = 99
            if attack == "missing_issue": scenario["metadata"]["issue"] = None
            if attack == "missing_description": scenario["metadata"]["description"] = " "
            if attack == "missing_rules": scenario["metadata"]["rules"] = []
            if attack == "missing_fixed": scenario["metadata"]["fixed_in"] = None
            if attack == "digest_only":
                scenario["expected"]["life"] = {}; scenario["expected"]["zones"] = []
                for action in scenario["actions"]:
                    action["expected_choice"] = None; action["expected_rejection"] = False
            original = json.dumps(scenario, indent=2).encode()
            candidate.write_bytes(original)
            review["scenario_sha256"] = hashlib.sha256(original).hexdigest()
            if attack == "wrong_hash": review["scenario_sha256"] = "0" * 64
            review_bytes = json.dumps(review).encode(); review_path.write_bytes(review_bytes)
            calls = []
            def execute(command, logpath, env, timeout):
                calls.append(command)
                self.assertEqual(command[1:3], ["scenario", "run"], "intake must never record/rebase")
                self.assertNotEqual(Path(command[-1]), candidate, "execute only private snapshot")
                logpath.parent.mkdir(parents=True, exist_ok=True)
                observation = {"pass": attack != "observed_fail", "digest": "synthetic-observed-digest",
                               "first_divergent_action": None, "message": "PASS",
                               "expected_state": None, "actual_state": None, "diff": []}
                if attack == "missing_digest": observation.pop("digest")
                if attack == "blank_digest": observation["digest"] = ""
                logpath.write_text("INVALID JSON" if attack == "malformed_log" else json.dumps(observation))
                if attack == "modify_snapshot": Path(command[-1]).write_text("mutated private scenario")
                result = {"command": command, "status": "PASS", "exit_code": 0, "log": str(logpath)}
                if attack == "nonzero_exit": result["exit_code"] = 1
                if attack == "boolean_exit": result["exit_code"] = False
                if attack == "timeout": result.update(status="TIMEOUT", exit_code=None)
                if attack == "start_error": result.update(status="ERROR", exit_code=None, error="start failed")
                return result
            result = load_intake().prepare("mock-engine", candidate, review_path, output, {}, 30, execute)
            self.assertEqual(candidate.read_bytes(), original)
            self.assertEqual(review_path.read_bytes(), review_bytes)
            self.assertLessEqual(len(calls), 1)
            bundles = [p for p in output.glob("issue_*.json")] if output.exists() else []
            if bundles:
                self.assertEqual(bundles[0].read_bytes(), original, "staging must preserve reviewed raw candidate bytes")
            return result, bundles and bundles[0].read_bytes(), calls, [p.suffix for p in output.glob("issue_*.*")]

    def prepared(self, result):
        return result.get("status") == "PREPARED" or result.get("accepted") is True

    def test_reviewed_behavioral_candidate_stages_exact_bytes_and_advisory_review(self):
        result, fixture, calls, suffixes = self.exercise()
        self.assertTrue(self.prepared(result), result)
        self.assertTrue(fixture)
        self.assertEqual(len(calls), 1)
        self.assertIn(".review", suffixes)
        self.assertFalse(result.get("confidence_promotion", False))
        self.assertFalse(result.get("verified", False))

    def test_invalid_review_or_metadata_rejects_before_running_engine(self):
        for attack in ["same_reviewer", "blank_builder", "blank_reviewer", "blank_evidence", "blank_behavior",
                       "wrong_kind", "wrong_format", "wrong_hash", "missing_issue", "missing_description",
                       "missing_rules", "missing_fixed", "digest_only"]:
            with self.subTest(attack=attack):
                result, fixture, calls, _ = self.exercise(attack)
                self.assertFalse(self.prepared(result), result)
                self.assertFalse(fixture)
                self.assertFalse(calls)

    def test_untrustworthy_execution_never_stages_a_regression(self):
        for attack in ["observed_fail", "missing_digest", "blank_digest", "malformed_log", "nonzero_exit",
                       "boolean_exit", "timeout", "start_error", "modify_snapshot"]:
            with self.subTest(attack=attack):
                result, fixture, _, _ = self.exercise(attack)
                self.assertFalse(self.prepared(result), result)
                self.assertFalse(fixture)

    def test_existing_staging_bundle_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); output = root / "staging"; output.mkdir()
            sentinel = output / "issue_example.json"; sentinel.write_text("existing regression")
            def execute(*args): self.fail("nonempty staging must reject before command")
            result = load_intake().prepare("mock", root / "missing.json", root / "missing.review", output, {}, 30, execute)
            self.assertFalse(self.prepared(result))
            self.assertEqual(sentinel.read_text(), "existing regression")
