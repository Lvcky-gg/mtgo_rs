"""Independent triage contract attacks; synthetic mocked reports are not rule evidence."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/verification_triage.py"


def load_triage():
    spec = importlib.util.spec_from_file_location("triage_review", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class TriageReview(unittest.TestCase):
    def exercise(self, attack=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "original.json"
            scenario = json.loads((ROOT / "tests/replays/basic_casting.json").read_text())
            original.write_text(json.dumps(scenario))
            immutable = original.read_bytes()
            output = root / "triage"
            calls = []
            def execute(command, logpath, env, timeout):
                calls.append((command, timeout))
                logpath.parent.mkdir(parents=True, exist_ok=True)
                logpath.write_text("mock command diagnostic\n")
                status, code = "PASS", 0
                if len(calls) == 1 and attack in ("timeout", "start_error"):
                    return {"command": command, "status": "TIMEOUT" if attack == "timeout" else "ERROR", "exit_code": None, "log": str(logpath), "error": "mock execution failure"}
                if command[1:3] == ["scenario", "report"]:
                    path = Path(command[-2]); artifact = json.loads(path.read_text())
                    minimized = path.name == "minimized.json"
                    index = len(artifact["actions"]) - 1
                    observed = {"pass": attack == "passing_original" and not minimized,
                                "first_divergent_action": index,
                                "message": "First divergent checkpoint",
                                "digest": "synthetic-observed-digest",
                                "expected_state": None, "actual_state": None,
                                "diff": ["/state/priority: expected 0, actual 1"]}
                    if minimized and attack == "wrong_message": observed["message"] = "Illegal recorded answer"
                    if minimized and attack == "wrong_path": observed["diff"] = ["/state/life: expected 20, actual 19"]
                    report = {"format_version": 1, "scenario": artifact, "observed": observed}
                    if len(calls) == 1 and attack == "missing_report": pass
                    elif len(calls) == 1 and attack == "malformed_report": Path(command[-1]).write_text("INVALID JSON")
                    else: Path(command[-1]).write_text(json.dumps(report))
                    status, code = ("PASS", 0) if observed["pass"] else ("FAIL", 1)
                    if len(calls) == 1 and attack == "capture_exit_two": code = 2
                    if len(calls) == 1 and attack == "capture_boolean_exit": code = True
                elif command[1:3] == ["report", "replay"]:
                    captured = json.loads(Path(command[-1]).read_text())
                    logpath.write_text(json.dumps({"reproduced": True, "observed": captured["observed"]}))
                    if attack == "replay_mismatch": status, code = "FAIL", 1
                    if attack == "replay_bad_json": logpath.write_text("INVALID JSON")
                    if attack == "replay_wrong_observation":
                        changed = copy.deepcopy(captured["observed"]); changed["digest"] = "different"
                        logpath.write_text(json.dumps({"reproduced": True, "observed": changed}))
                    if attack == "replay_boolean_exit": code = False
                elif command[1:3] == ["scenario", "minimize"]:
                    reduced = copy.deepcopy(scenario)
                    reduced["actions"] = reduced["actions"][1:]
                    if attack == "wrong_action": reduced["actions"][-1]["answer"] = "forged-action"
                    if attack == "rewrite_metadata": reduced["metadata"]["issue"] = "forged-provenance"
                    if attack == "rewrite_assertions": reduced["expected"]["life"] = {"0": 999}
                    if attack == "rewrite_players": reduced["initial_state"]["players"][0]["life"] += 1
                    if attack == "rewrite_budget": reduced["advance_budget"] = 1
                    if attack == "rewrite_original": Path(command[-2]).write_text("mutated original input")
                    Path(command[-1]).write_text(json.dumps(reduced))
                return {"command": command, "status": status, "exit_code": code, "log": str(logpath)}
            result = load_triage().triage("mock-binary", original, output, {}, 999, execute)
            self.assertEqual(original.read_bytes(), immutable)
            self.assertLessEqual(len(calls), 5)
            self.assertTrue(all(timeout <= 30 for _, timeout in calls))
            return result, calls

    def minimized(self, result):
        return result.get("status") == "MINIMIZED" or result.get("accepted") is True

    def test_matching_reduction_accepts_shifted_index_but_same_action_and_diff_path(self):
        result, calls = self.exercise()
        self.assertTrue(self.minimized(result), result)
        self.assertEqual(len(calls), 5)
        self.assertFalse(result.get("regression_promotion", False))
        self.assertFalse(result.get("confidence_promotion", False))

    def test_missing_malformed_passing_and_unreproduced_original_never_minimize(self):
        for attack in ["missing_report", "malformed_report", "passing_original", "replay_mismatch", "timeout", "start_error", "capture_exit_two", "capture_boolean_exit", "replay_bad_json", "replay_wrong_observation", "replay_boolean_exit"]:
            with self.subTest(attack=attack):
                result, calls = self.exercise(attack)
                self.assertFalse(self.minimized(result), result)
                self.assertFalse(any(c[0][1:3] == ["scenario", "minimize"] for c in calls))

    def test_wrong_failure_message_action_or_canonical_path_rejects_reduction(self):
        for attack in ["wrong_message", "wrong_action", "wrong_path"]:
            with self.subTest(attack=attack):
                result, _ = self.exercise(attack)
                self.assertFalse(self.minimized(result), result)

    def test_reducer_cannot_rewrite_assertions_provenance_or_nonobject_setup(self):
        for attack in ["rewrite_metadata", "rewrite_assertions", "rewrite_players", "rewrite_budget"]:
            with self.subTest(attack=attack):
                result, _ = self.exercise(attack)
                self.assertFalse(self.minimized(result), result)

    def test_misbehaving_reducer_cannot_modify_external_original_input(self):
        result, _ = self.exercise("rewrite_original")
        self.assertFalse(self.minimized(result), result)

    def test_missing_serialized_input_skips_all_commands(self):
        with tempfile.TemporaryDirectory() as directory:
            def execute(*args): self.fail("missing input must not invoke commands")
            result = load_triage().triage("mock", Path(directory) / "missing.json", Path(directory) / "evidence", {}, 30, execute)
            self.assertEqual(result["status"], "SKIPPED")

    def test_json_reads_are_bounded_before_parse(self):
        module = load_triage()
        module.MAX_JSON_BYTES = 8
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "oversize.json"; path.write_bytes(b" " * 9)
            with self.assertRaises(ValueError): module.read_json(path)

    def test_preexisting_evidence_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); output = root / "triage"; output.mkdir()
            sentinel = output / "original_report.json"; sentinel.write_text("original evidence")
            original = root / "original.json"; original.write_text("{}")
            def execute(*args): self.fail("nonempty output must be rejected before commands")
            try:
                result = load_triage().triage("mock-binary", original, output, {}, 30, execute)
                self.assertFalse(self.minimized(result))
            except (ValueError, FileExistsError):
                pass
            self.assertEqual(sentinel.read_text(), "original evidence")
