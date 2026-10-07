#!/usr/bin/env python3
"""Run real production mutations in a disposable source copy.

This is a curated milestone smoke campaign, not a repository mutation score.
Compilation failures and missing tests are errors, never killed mutants.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MUTATIONS = [
    {
        "id": "sba_zero_toughness_boundary",
        "source": "crates/mtg-engine/src/sba.rs",
        "before": "if toughness <= 0 {",
        "after": "if toughness < 0 {",
        "checks": [
            ["--lib", "production_sba_matches_independent_table_across_small_state_space"],
            ["--test", "rules", "cr_704_5f_zero_toughness_is_an_owner_graveyard_move_even_if_indestructible"],
            ["--test", "properties", "stable_priority_never_retains_nonpositive_toughness"],
        ],
    },
    {
        "id": "opponent_hand_identity_leak",
        "source": "crates/mtg-engine/src/view.rs",
        "before": "Zone::Hand => obj.owner == viewer,",
        "after": "Zone::Hand => true,",
        "checks": [["--test", "properties", "p0_hidden_hand_and_library_identity_noninterference"]],
    },
    {
        "id": "shared_command_zone_ignores_owner",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "o.owner == who && self.state.commander.is_commander(o.owner, o.card)",
        "after": "self.state.commander.is_commander(o.owner, o.card)",
        "checks": [["--test", "commander", "cr_903_8_shared_command_zone_casting_is_owner_only"]],
    },
    {
        "id": "commander_return_confirmation_inverted",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "if accept {\n                    actions.push(Sba::ReturnCommander { object });",
        "after": "if !accept {\n                    actions.push(Sba::ReturnCommander { object });",
        "checks": [["--test", "commander", "cr_903_9a_owner_can_accept_or_decline_graveyard_and_exile_returns"]],
    },
]


def run(copy, env, args, timeout):
    process = subprocess.run(
        ["cargo", "test", "-p", "mtg-verify", "--offline", *args],
        cwd=copy, env=env, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, timeout=timeout, check=False,
    )
    return {"command": args, "exit_code": process.returncode, "output": process.stdout}


def test_outcome(result):
    match = re.search(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;", result["output"])
    if not match or sum(map(int, match.groups())) == 0:
        return "ERROR"
    if result["exit_code"] == 0 and int(match[2]) == 0:
        return "PASS"
    if result["exit_code"] != 0 and int(match[2]) > 0:
        return "FAIL"
    return "ERROR"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target" / "verification-mutations")
    parser.add_argument("--timeout", type=int, default=300)
    parser.add_argument("--cases", type=int, default=1000)
    args = parser.parse_args()
    if args.cases < 1:
        parser.error("--cases must be positive")
    report = {"format_version": 1, "scope": "four selected production mutants; not repository mutation score", "property_cases_per_check": args.cases, "mutations": []}
    env = dict(os.environ, CARGO_TARGET_DIR=str(args.target_dir.resolve()), PROPTEST_CASES=str(args.cases), PROPTEST_RNG_SEED="481", PROPTEST_DISABLE_FAILURE_PERSISTENCE="1")
    try:
        with tempfile.TemporaryDirectory(prefix="mtgo-production-mutations-") as temporary:
            copy = Path(temporary)
            for filename in ["Cargo.toml", "Cargo.lock"]:
                shutil.copy2(ROOT / filename, copy / filename)
            shutil.copytree(ROOT / "crates", copy / "crates", ignore=shutil.ignore_patterns("target", "proptest-regressions"))
            digest = hashlib.sha256()
            for source in sorted(copy.rglob("*.rs")):
                digest.update(str(source.relative_to(copy)).encode())
                digest.update(b"\0")
                digest.update(source.read_bytes())
            report["rust_source_sha256"] = digest.hexdigest()
            # Test fixtures referenced outside crate roots must accompany the copy.
            if (ROOT / "tests").exists():
                shutil.copytree(ROOT / "tests", copy / "tests")
            if (ROOT / "fuzz" / "corpus").exists():
                shutil.copytree(ROOT / "fuzz" / "corpus", copy / "fuzz" / "corpus")
            if (ROOT / "verification").exists():
                shutil.copytree(ROOT / "verification", copy / "verification")
            # Validate every check against the unchanged baseline first.
            for mutation in MUTATIONS:
                for check in mutation["checks"]:
                    baseline = run(copy, env, check, args.timeout)
                    if test_outcome(baseline) != "PASS":
                        raise RuntimeError("baseline check failed: " + json.dumps(baseline))
            for mutation in MUTATIONS:
                source = copy / mutation["source"]
                original = source.read_text()
                if original.count(mutation["before"]) != 1:
                    raise RuntimeError("mutation anchor must appear exactly once: " + mutation["id"])
                result = {"id": mutation["id"], "source": mutation["source"], "before": mutation["before"], "after": mutation["after"], "checks": []}
                try:
                    source.write_text(original.replace(mutation["before"], mutation["after"]))
                    compilation = run(copy, env, ["--lib", "--tests", "--no-run"], args.timeout)
                    if compilation["exit_code"] != 0:
                        result.update(status="ERROR", compilation=compilation)
                    else:
                        for check in mutation["checks"]:
                            observed = run(copy, env, check, args.timeout)
                            observed["status"] = test_outcome(observed)
                            result["checks"].append(observed)
                        statuses = [check["status"] for check in result["checks"]]
                        result["status"] = "ERROR" if "ERROR" in statuses else "KILLED" if "FAIL" in statuses else "SURVIVED"
                finally:
                    source.write_text(original)
                report["mutations"].append(result)
    except (OSError, subprocess.TimeoutExpired, RuntimeError) as error:
        report["error"] = str(error)
    report["killed"] = sum(m["status"] == "KILLED" for m in report["mutations"])
    report["completed"] = len(report["mutations"])
    report["passed"] = "error" not in report and report["completed"] == len(MUTATIONS) and all(m["status"] == "KILLED" for m in report["mutations"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Production mutation smoke: {report['killed']}/{len(MUTATIONS)} killed; report: {args.output}")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
