"""Bounded private failure intake; reductions are candidates, never promoted regressions."""
import json
from pathlib import Path

MAX_JSON_BYTES = 64 * 1024 * 1024


def read_bytes(path):
    with Path(path).open("rb") as source:
        data = source.read(MAX_JSON_BYTES + 1)
    if len(data) > MAX_JSON_BYTES:
        raise ValueError("triage artifact exceeds size limit")
    return data


def read_json(path):
    return json.loads(read_bytes(path))


def signature(report):
    observed, scenario = report["observed"], report["scenario"]
    message = observed["message"]
    if not isinstance(message, str) or not message:
        raise ValueError("missing observed failure message")
    index = observed["first_divergent_action"]
    actions = scenario["actions"]
    if index is not None and (type(index) is not int or index < 0 or index > len(actions)):
        raise ValueError("invalid divergent action")
    action = actions[index] if index is not None and index < len(actions) else None
    paths = [line.split(": expected ", 1)[0] for line in observed["diff"]] if message in ("First divergent checkpoint", "Final state differs") else []
    return message.split(", actual ", 1)[0], action, paths


def triage(binary, scenario_path, output_dir, env, timeout, execute):
    scenario_path, output = Path(scenario_path), Path(output_dir)
    result = {"status": "ERROR", "commands": [], "confidence_promotion": False,
              "regression_promotion": False}
    snapshot = output / "input.json"
    original_bytes = None

    def command(stage, args):
        entry = execute([str(binary), *map(str, args)], output / (stage + ".log"), env, min(timeout, 30))
        result["commands"].append(dict(entry, stage=stage))
        if read_bytes(snapshot) != original_bytes:
            raise ValueError("private failure input changed during triage")
        return entry

    def success(entry, status, code):
        return entry.get("status") == status and type(entry.get("exit_code")) is int and entry["exit_code"] == code and "error" not in entry

    def capture(path, name):
        attachment = output / (name + "_report.json")
        entry = command(name + "_capture", ["scenario", "report", path, attachment])
        if not attachment.is_file():
            raise ValueError("failure capture did not create an attachment")
        report = read_json(attachment)
        if report["observed"]["pass"] is not False or not success(entry, "FAIL", 1):
            raise ValueError("capture is not a confirmed failed replay")
        replay = command(name + "_replay", ["report", "replay", attachment])
        if not success(replay, "PASS", 0):
            raise ValueError("captured failure did not reproduce")
        observation = read_json(output / (name + "_replay.log"))
        if observation.get("reproduced") is not True or observation.get("observed") != report["observed"]:
            raise ValueError("replay observation differs from captured failure")
        result[name + "_report"] = str(attachment)
        return report

    try:
        output.mkdir(parents=True, exist_ok=True)
        if any(output.iterdir()):
            raise ValueError("triage evidence directory must be empty")
        if not scenario_path.is_file():
            result.update(status="SKIPPED", error="no serialized failure scenario")
            return result
        original_bytes = read_bytes(scenario_path)
        # Give potentially faulty reducers a separate file, never the caller's
        # original failure artifact. Monitor that private snapshot after each step.
        with snapshot.open("xb") as copy:
            copy.write(original_bytes)
        result["source_snapshot"] = str(snapshot)
        original = capture(snapshot, "original")
        result["original_reproduced"] = True
        minimized = output / "minimized.json"
        entry = command("minimize", ["scenario", "minimize", snapshot, minimized])
        if not success(entry, "PASS", 0):
            raise ValueError("failure minimization did not complete")
        reduced = capture(minimized, "minimized")
        if signature(original) != signature(reduced):
            raise ValueError("minimization substituted a different failure")
        # Compare normalized scenario fields from capture, preserving explicit rules
        # assertions/provenance even when source JSON omitted default fields.
        for key in set(original["scenario"]) | set(reduced["scenario"]):
            if key not in ("actions", "initial_state") and original["scenario"].get(key) != reduced["scenario"].get(key):
                raise ValueError("minimization rewrote assertions or provenance")
        for key in set(original["scenario"]["initial_state"]) | set(reduced["scenario"]["initial_state"]):
            if key != "objects" and original["scenario"]["initial_state"].get(key) != reduced["scenario"]["initial_state"].get(key):
                raise ValueError("minimization rewrote non-object setup")
        if read_bytes(scenario_path) != original_bytes:
            raise ValueError("original failure scenario changed during triage")
        result.update(status="MINIMIZED", minimized=str(minimized))
    except (OSError, ValueError, TypeError, KeyError, AttributeError, RecursionError) as error:
        result["error"] = str(error)
    return result
