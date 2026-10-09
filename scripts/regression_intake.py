#!/usr/bin/env python3
"""Stage reviewed, passing regression fixtures without rewriting expectations."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verification_triage import read_bytes


def prepare(binary, scenario_path, review_path, output_dir, env, timeout, execute):
    source, review_source, output = Path(scenario_path), Path(review_path), Path(output_dir)
    result = {"status": "ERROR", "commands": [], "confidence_promotion": False,
              "regression_promotion": False, "verified": False}
    try:
        output.mkdir(parents=True, exist_ok=True)
        if any(output.iterdir()):
            raise ValueError("staging directory must be empty")
        if type(timeout) not in (int, float) or not 0 < timeout <= 3600:
            raise ValueError("invalid command timeout")
        candidate_bytes, review_bytes = read_bytes(source), read_bytes(review_source)
        scenario, review = json.loads(candidate_bytes), json.loads(review_bytes)
        metadata = scenario["metadata"]
        for key in ("issue", "description", "fixed_in"):
            if not isinstance(metadata.get(key), str) or not metadata[key].strip():
                raise ValueError("missing regression metadata: " + key)
        rules = metadata.get("rules")
        if not isinstance(rules, list) or not rules or any(not isinstance(rule, str) or not rule.strip() for rule in rules):
            raise ValueError("missing rules references")
        if type(review.get("format_version")) is not int or review["format_version"] != 1:
            raise ValueError("unsupported review format")
        for key in ("builder", "reviewer", "expected_behavior", "evidence"):
            if not isinstance(review.get(key), str) or not review[key].strip():
                raise ValueError("missing independent review field: " + key)
        if review["builder"].strip().casefold() == review["reviewer"].strip().casefold():
            raise ValueError("builder cannot be the independent reviewer")
        if review.get("kind") not in ("engine", "harness"):
            raise ValueError("invalid review kind")
        digest = hashlib.sha256(candidate_bytes).hexdigest()
        if review.get("scenario_sha256") != digest:
            raise ValueError("review does not match candidate bytes")
        expected = scenario.get("expected", {})
        direct = bool(expected.get("life") or expected.get("zones")) or any(
            action.get("expected_choice") is not None or action.get("expected_rejection") is True
            for action in scenario.get("actions", []))
        if not direct:
            raise ValueError("regression requires a direct behavioral assertion")
        slug = re.sub(r"[^a-zA-Z0-9_-]+", "_", metadata["issue"]).strip("_")[:100]
        if not slug:
            raise ValueError("issue cannot form a safe fixture name")
        snapshot = output / "input.scenario"
        with snapshot.open("xb") as handle:
            handle.write(candidate_bytes)
        log = output / "validation.log"
        entry = execute([str(binary), "scenario", "run", str(snapshot)], log, env, min(timeout, 30))
        result["commands"].append(entry)
        if (entry.get("status") != "PASS" or type(entry.get("exit_code")) is not int
                or entry["exit_code"] != 0 or "error" in entry):
            raise ValueError("candidate validation did not pass")
        observed = json.loads(read_bytes(log))
        if (observed.get("pass") is not True or not isinstance(observed.get("digest"), str)
                or not observed["digest"].strip() or observed.get("first_divergent_action") is not None
                or observed.get("diff") != []):
            raise ValueError("candidate did not report an unambiguous passing observation")
        if read_bytes(snapshot) != candidate_bytes or read_bytes(source) != candidate_bytes or read_bytes(review_source) != review_bytes:
            raise ValueError("candidate or review changed during validation")
        fixture = output / ("issue_" + slug + ".json")
        sidecar = fixture.with_suffix(".review")
        with sidecar.open("xb") as handle:
            handle.write(review_bytes)
        with fixture.open("xb") as handle:
            handle.write(candidate_bytes)
        result.update(status="PREPARED", fixture=str(fixture), review=str(sidecar),
                      scenario_sha256=digest, observed=observed)
    except (OSError, ValueError, TypeError, KeyError, AttributeError, RecursionError) as error:
        result["error"] = str(error)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario", type=Path, required=True)
    parser.add_argument("--review", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=Path(__file__).resolve().parents[1] / "target/debug/mtgo-rs")
    parser.add_argument("--timeout", type=int, default=30)
    args = parser.parse_args()
    from verification_campaign import execute
    result = prepare(args.binary, args.scenario, args.review, args.output, dict(os.environ), args.timeout, execute)
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "PREPARED" else 1


if __name__ == "__main__":
    raise SystemExit(main())
