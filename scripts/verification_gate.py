#!/usr/bin/env python3
"""Fail closed for release; empty issue lists are not evidence of an assessment."""
import argparse
import json
from pathlib import Path

REQUIRED = ("state_corruption", "network_divergence", "silent_unsupported_rules")


def evaluate(manifest, campaign, release=False):
    failures = []
    observed = {}
    if manifest.get("format_version") != 1:
        failures.append("unsupported issue-manifest version")
    checks = campaign.get("checks", [])
    if not isinstance(checks, list) or any(not isinstance(c, dict) for c in checks):
        checks = []
    required_checks = {"verification_tests", "build_replay_cli", "replays_0", "regressions_0", "semantic_0"}
    if not required_checks.issubset({c.get("name") for c in checks}):
        failures.append("missing required observed verification families")
    if campaign.get("format_version") != 1 or campaign.get("passed") is not True or not checks:
        failures.append("verification campaign did not pass with observed checks")
    if any(check.get("status") != "PASS" for check in checks):
        failures.append("campaign contains unsuccessful checks")
    issues = manifest.get("known_issues", {})
    if not isinstance(issues, dict):
        issues = {}
    for name in REQUIRED:
        entry = issues.get(name)
        if not isinstance(entry, dict) or not isinstance(entry.get("open"), list):
            failures.append("missing or malformed issue category: " + name)
            observed[name] = None
            continue
        if entry.get("assessment") not in ("reviewed", "unassessed"):
            failures.append("invalid assessment state: " + name)
        def nonempty_text(field):
            value = entry.get(field)
            return isinstance(value, str) and bool(value.strip())
        assessed = entry.get("assessment") == "reviewed" and nonempty_text("reviewed_by") and nonempty_text("evidence")
        observed[name] = len(entry["open"]) if assessed else None
        if entry["open"]:
            failures.append("known blocking violations: " + name)
        if release and not assessed:
            failures.append("release assessment unknown: " + name)
    return {"format_version": 1, "passed": not failures, "release_gate": release,
            "known_blocking_counts": observed, "failures": failures, "confidence_promotion": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=Path("verification.json"))
    parser.add_argument("--campaign", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--release", action="store_true")
    args = parser.parse_args()
    try:
        report = evaluate(json.loads(args.manifest.read_text()), json.loads(args.campaign.read_text()), args.release)
    except (OSError, ValueError, TypeError) as error:
        report = {"format_version": 1, "passed": False, "failures": [str(error)]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
