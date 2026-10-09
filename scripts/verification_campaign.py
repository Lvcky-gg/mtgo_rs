#!/usr/bin/env python3
"""Bounded verification campaigns; archive observed outcomes, never invented evidence."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verification_evidence import MAX_SEMANTIC_GAMES, fingerprint, plan
from verification_triage import triage

ROOT = Path(__file__).resolve().parents[1]
TIERS = {"pr": (10_000, 4, 200), "nightly": (1_000_000, 100, 2_000), "weekly": (2_000_000, 1_000, 10_000)}


def execute(command, output, env, timeout):
    started = time.monotonic()
    with output.open("w") as log:
        try:
            process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log,
                                       stderr=subprocess.STDOUT, start_new_session=True)
        except OSError as failure:
            error = str(failure)
            log.write(error + "\n")
            return {"command": command, "status": "ERROR", "exit_code": None,
                    "elapsed_seconds": round(time.monotonic() - started, 3),
                    "log": str(output), "error": error}
        try:
            code = process.wait(timeout=timeout)
            status = "PASS" if code == 0 else "FAIL"
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            code, status = None, "TIMEOUT"
    return {"command": command, "status": status, "exit_code": code,
            "elapsed_seconds": round(time.monotonic() - started, 3), "log": str(output)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tier", choices=TIERS, default="pr")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=int, help="per-command cap; defaults to 1200s PR / 7200s scheduled")
    parser.add_argument("--semantic-games", type=int)
    args = parser.parse_args()
    if (args.timeout is not None and args.timeout < 1) or (args.semantic_games is not None and not 1 <= args.semantic_games <= MAX_SEMANTIC_GAMES):
        parser.error("budgets must be positive")
    timeout = args.timeout or (1200 if args.tier == "pr" else 7200)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error("evidence output directory must be empty; choose a new directory")
    cases, games, actions = TIERS[args.tier]
    games = args.semantic_games or games
    env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / "target"), PROPTEST_CASES=str(cases), PROPTEST_RNG_SEED="481",
               MTGO_VERIFY_ACTIONS=str(actions))
    report = {"format_version": 1, "tier": args.tier, "property_cases_requested_per_property": cases,
              "semantic_action_budget_per_game": actions, "checks": [],
              "unsupported_campaigns": ["real-deck network impairment campaigns", "external engine differential testing"],
              "confidence_promotion": False, "failure_triage": []}

    def diagnose(path, name):
        try:
            diagnosis = triage(binary, path, output / ("triage_" + name), env, timeout, execute)
        except Exception as error:
            diagnosis = {"status": "ERROR", "error": str(error), "commands": [],
                         "confidence_promotion": False, "regression_promotion": False}
        report["failure_triage"].append(dict(diagnosis, source=str(path)))

    def check(name, command):
        result = execute(command, output / (name + ".log"), env, timeout)
        result["name"] = name
        report["checks"].append(result)
        return result["status"] == "PASS"

    try:
        report["source_fingerprint"] = fingerprint(ROOT)
        report["plan"] = plan(ROOT, games, args.tier)
        # One complete crate run includes independent golden/property/canonical tests
        # and the primitive differential smoke tests. Budgets are requests, not claims.
        if not check("verification_tests", ["cargo", "test", "-p", "mtg-verify", "--locked"]):
            raise RuntimeError("verification tests failed")
        if not check("build_replay_cli", ["cargo", "build", "-p", "mtg-verify", "--bin", "mtgo-rs", "--locked"]):
            raise RuntimeError("replay CLI build failed")
        binary = ROOT / "target" / "debug" / "mtgo-rs"
        corpora = {}
        for corpus in ("replays", "regressions"):
            fixtures = [ROOT / path for path in report["plan"]["corpora"][corpus]]
            if not fixtures:
                raise RuntimeError("required corpus is empty: " + corpus)
            corpora[corpus] = fixtures
            for index, fixture in enumerate(fixtures):
                if not check(f"{corpus}_{index}", [str(binary), "replay", str(fixture)]):
                    diagnose(fixture, f"{corpus}_{index}")
                    raise RuntimeError("corpus fixture failed: " + str(fixture))
        templates = corpora["replays"]
        for index in range(games):
            template = json.loads(templates[index % len(templates)].read_text())
            template["seed"] = 481 + index
            seed_path = output / f"semantic_seed_{index}.json"
            seed_path.write_text(json.dumps(template, indent=2) + "\n")
            reproduction = output / f"semantic_result_{index}.json"
            if not check(f"semantic_{index}", [str(binary), "scenario", "fuzz", str(seed_path),
                                              str(reproduction)]):
                diagnose(reproduction, f"semantic_{index}")
                raise RuntimeError("semantic campaign failed; seed and reproduction retained")
            report["checks"][-1]["semantic_actions_observed"] = len(json.loads(reproduction.read_text())["actions"])
            if not check(f"semantic_replay_{index}", [str(binary), "replay", str(reproduction)]):
                diagnose(reproduction, f"semantic_replay_{index}")
                raise RuntimeError("generated semantic artifact did not replay deterministically")
        if args.tier == "weekly":
            if not check("production_mutations", ["python3", "scripts/verify_mutations.py", "--output",
                                                  str(output / "mutations.json"), "--cases", "10000"]):
                raise RuntimeError("production mutation campaign failed")
            if not check("gate_mutations", ["python3", "scripts/verify_gate_mutations.py", "--output",
                                           str(output / "gate_mutations.json")]):
                raise RuntimeError("verification gate mutation campaign failed")
        if fingerprint(ROOT) != report["source_fingerprint"]:
            raise RuntimeError("verification sources changed during campaign")
    except (OSError, RuntimeError, ValueError, TypeError, KeyError) as error:
        report["error"] = str(error)
    report["semantic_actions_observed"] = sum(c.get("semantic_actions_observed", 0) for c in report["checks"])
    report["passed"] = "error" not in report and all(c["status"] == "PASS" for c in report["checks"])
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"passed": report["passed"], "checks": len(report["checks"]), "report": str(output / "report.json")}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
