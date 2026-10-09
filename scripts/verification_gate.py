#!/usr/bin/env python3
"""Fail closed for release; empty issue lists are not evidence of an assessment."""
import argparse
import json
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verification_evidence import fingerprint, plan, planned_checks

ROOT = Path(__file__).resolve().parents[1]

REQUIRED = ("state_corruption", "network_divergence", "silent_unsupported_rules")


def evaluate(manifest, campaign, release=False, expected_fingerprint=None, expected_plan=None):
    failures = []
    observed = {}
    if not isinstance(manifest, dict) or not isinstance(campaign, dict):
        return {"format_version": 1, "passed": False, "release_gate": release,
                "known_blocking_counts": {}, "failures": ["malformed gate inputs"],
                "confidence_promotion": False}
    if type(manifest.get("format_version")) is not int or manifest.get("format_version") != 1:
        failures.append("unsupported issue-manifest version")
    checks = campaign.get("checks", [])
    if not isinstance(checks, list) or any(not isinstance(c, dict) for c in checks):
        checks = []
    required_checks = {"verification_tests", "build_replay_cli", "replays_0", "regressions_0", "semantic_0", "semantic_replay_0"}
    if not required_checks.issubset({c.get("name") for c in checks if isinstance(c.get("name"), str)}):
        failures.append("missing required observed verification families")
    if type(campaign.get("format_version")) is not int or campaign.get("format_version") != 1 or campaign.get("passed") is not True or not checks:
        failures.append("verification campaign did not pass with observed checks")
    if any(check.get("status") != "PASS" for check in checks):
        failures.append("campaign contains unsuccessful checks")
    if "error" in campaign or any("error" in check for check in checks):
        failures.append("campaign contains execution errors")
    diagnostics = campaign.get("failure_triage", [])
    if not isinstance(diagnostics, list) or diagnostics:
        failures.append("campaign contains failure triage evidence")
    if any(type(c.get("exit_code")) is not int or c["exit_code"] != 0 for c in checks):
        failures.append("campaign checks lack successful exit codes")
    names = [c.get("name") for c in checks]
    if any(not isinstance(name, str) or not name for name in names) or len(set(n for n in names if isinstance(n, str))) != len(names):
        failures.append("invalid or duplicate check names")
    source = campaign.get("source_fingerprint")
    if not isinstance(source, str) or not re.fullmatch(r"[0-9a-f]{64}", source):
        failures.append("missing or invalid source fingerprint")
    elif expected_fingerprint is not None and source != expected_fingerprint:
        failures.append("campaign belongs to different verification sources")
    inventory = campaign.get("plan")
    try:
        if not isinstance(inventory, dict):
            raise ValueError("missing campaign plan")
        expected = planned_checks(inventory.get("corpora"), inventory.get("semantic_games"), campaign.get("tier"))
        if inventory.get("checks") != expected:
            raise ValueError("inconsistent campaign plan")
        if sorted(n for n in names if isinstance(n, str)) != sorted(expected):
            raise ValueError("campaign did not complete its entire planned inventory")
        if expected_plan is not None and inventory != expected_plan:
            raise ValueError("campaign corpus differs from current inventory")
    except (ValueError, TypeError) as error:
        failures.append(str(error))
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
        campaign = json.loads(args.campaign.read_text())
        expected_plan = plan(ROOT, campaign.get("plan", {}).get("semantic_games"), campaign.get("tier")) if isinstance(campaign, dict) and isinstance(campaign.get("plan"), dict) else None
        report = evaluate(json.loads(args.manifest.read_text()), campaign, args.release,
                          expected_fingerprint=fingerprint(ROOT), expected_plan=expected_plan)
    except (OSError, ValueError, TypeError) as error:
        report = {"format_version": 1, "passed": False, "failures": [str(error)]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
