import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "verification_gate.py"
spec = importlib.util.spec_from_file_location("verification_gate", SCRIPT)
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class ReleasePolicyTests(unittest.TestCase):
    def manifest(self, assessed=False):
        return {"format_version": 1, "known_issues": {
            name: {"assessment": "reviewed" if assessed else "unassessed",
                   "reviewed_by": "independent-reviewer" if assessed else None,
                   "evidence": "pinned-audit" if assessed else None, "open": []}
            for name in gate.REQUIRED}}

    def campaign(self):
        return {"format_version": 1, "passed": True,
                "checks": [{"name": name, "status": "PASS"}
                           for name in ("verification_tests", "build_replay_cli", "replays_0", "regressions_0", "semantic_0")]}

    def test_unknown_counts_are_not_zero_and_cannot_pass_closed_alpha(self):
        report = gate.evaluate(self.manifest(), self.campaign(), release=True)
        self.assertFalse(report["passed"])
        self.assertTrue(all(count is None for count in report["known_blocking_counts"].values()))

    def test_ordinary_release_remains_possible_without_claiming_verified(self):
        report = gate.evaluate(self.manifest(), self.campaign())
        self.assertTrue(report["passed"])
        self.assertFalse(report["confidence_promotion"])

    def test_known_issue_blocks_even_ordinary_release(self):
        manifest = self.manifest()
        manifest["known_issues"]["state_corruption"]["open"] = [{"issue": "example"}]
        self.assertFalse(gate.evaluate(manifest, self.campaign())["passed"])

    def test_failed_or_empty_campaign_cannot_pass(self):
        for campaign in ({"format_version": 1, "passed": True, "checks": []},
                         {"format_version": 1, "passed": True, "checks": [{"status": "FAIL"}]}):
            self.assertFalse(gate.evaluate(self.manifest(True), campaign, True)["passed"])

    def test_reviewed_zero_with_observed_passes_can_pass(self):
        report = gate.evaluate(self.manifest(True), self.campaign(), True)
        self.assertTrue(report["passed"])
        self.assertEqual(set(report["known_blocking_counts"].values()), {0})


if __name__ == "__main__":
    unittest.main()
