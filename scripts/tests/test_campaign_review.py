"""Independent release-judge attacks based on a real successful PR campaign."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("gate_review", ROOT / "scripts/verification_gate.py")
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class CampaignEvidenceReview(unittest.TestCase):
    def setUp(self):
        self.report = json.loads((Path(__file__).parent / "fixtures/pr_campaign_review.json").read_text())
        self.manifest = json.loads((ROOT / "verification.json").read_text())

    def accepts(self, report):
        return gate.evaluate(self.manifest, report)["passed"]

    def test_real_successful_campaign_is_accepted(self):
        self.assertTrue(self.accepts(self.report))

    def test_missing_generated_replay_cannot_be_called_success(self):
        self.report["checks"] = [c for c in self.report["checks"] if c["name"] != "semantic_replay_0"]
        self.assertFalse(self.accepts(self.report))

    def test_pass_with_nonzero_missing_or_boolean_exit_code_is_rejected(self):
        for value in [17, None, False]:
            report = copy.deepcopy(self.report)
            if value is None:
                report["checks"][0].pop("exit_code")
            else:
                report["checks"][0]["exit_code"] = value
            with self.subTest(exit_code=value):
                self.assertFalse(self.accepts(report))

    def test_duplicate_name_does_not_supply_extra_evidence(self):
        self.report["checks"].append(copy.deepcopy(self.report["checks"][0]))
        self.assertFalse(self.accepts(self.report))

    def test_top_level_error_overrides_claimed_pass(self):
        self.report["error"] = "interrupted after last completed command"
        self.assertFalse(self.accepts(self.report))

    def test_truncated_last_game_cannot_pass_just_because_game_zero_exists(self):
        self.report["checks"] = [c for c in self.report["checks"]
                                  if c["name"] not in ("semantic_3", "semantic_replay_3")]
        self.assertFalse(self.accepts(self.report))

    def test_truncated_corpus_cannot_pass_just_because_fixture_zero_exists(self):
        self.report["checks"] = [c for c in self.report["checks"] if c["name"] != "replays_7"]
        self.assertFalse(self.accepts(self.report))

    def test_success_status_with_check_error_is_not_execution_success(self):
        self.report["checks"][0]["error"] = "failed to start command"
        self.assertFalse(self.accepts(self.report))

    def test_failure_triage_cannot_promote_forged_passing_campaign(self):
        self.report["failure_triage"] = [{"status": "MINIMIZED", "passed": True,
                                          "commands": [], "confidence_promotion": False}]
        self.assertFalse(self.accepts(self.report))

    def test_ordering_of_completed_checks_is_irrelevant(self):
        self.report["checks"].reverse()
        self.assertTrue(self.accepts(self.report))


if __name__ == "__main__":
    unittest.main()

class CampaignProvenanceReview(CampaignEvidenceReview):
    def test_stale_fingerprint_rejected_when_current_source_is_required(self):
        self.assertFalse(gate.evaluate(self.manifest, self.report, expected_fingerprint="0" * 64)["passed"])

    def test_current_inventory_mismatch_rejected(self):
        expected = copy.deepcopy(self.report["plan"])
        expected["corpora"]["replays"].append("tests/replays/new_fixture.json")
        self.assertFalse(gate.evaluate(self.manifest, self.report, expected_plan=expected)["passed"])

    def test_weekly_cannot_skip_production_mutations(self):
        self.report["tier"] = "weekly"
        self.report["plan"]["checks"].append("production_mutations")
        self.assertFalse(self.accepts(self.report))

    def test_malformed_inputs_fail_closed_without_exception(self):
        for bad in [None, [], "PASS", 1]:
            self.assertFalse(gate.evaluate(self.manifest, bad)["passed"])
            self.assertFalse(gate.evaluate(bad, self.report)["passed"])
        for bad_name in [[], {}, 17, None, ""]:
            report = copy.deepcopy(self.report)
            report["checks"][0]["name"] = bad_name
            with self.subTest(name=bad_name):
                self.assertFalse(self.accepts(report))

    def test_malformed_plan_and_impossible_game_count_are_rejected(self):
        for games in [False, 0, -1, 100001, "4", None]:
            report = copy.deepcopy(self.report)
            report["plan"]["semantic_games"] = games
            self.assertFalse(self.accepts(report))
