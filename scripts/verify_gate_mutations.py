#!/usr/bin/env python3
"""Controlled gate/intake mutations; import/compile errors never count as kills."""
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

from verification_evidence import fingerprint

ROOT = Path(__file__).resolve().parents[1]
MUTATIONS = (
    ("truncated_campaign_accepted", "scripts/verification_gate.py", "test_campaign_review.py", "if sorted(n for n in names if isinstance(n, str)) != sorted(expected):", "if False:"),
    ("unsuccessful_exit_accepted", "scripts/verification_gate.py", "test_campaign_review.py", 'if any(type(c.get("exit_code")) is not int or c["exit_code"] != 0 for c in checks):', "if False:"),
    ("stale_source_accepted", "scripts/verification_gate.py", "test_campaign_review.py", "elif expected_fingerprint is not None and source != expected_fingerprint:", "elif False:"),
    ("triage_substitutes_failure", "scripts/verification_triage.py", "test_triage_review.py", "if signature(original) != signature(reduced):", "if False:"),
    ("triage_reducer_receives_original", "scripts/verification_triage.py", "test_triage_review.py", '["scenario", "minimize", snapshot, minimized]', '["scenario", "minimize", scenario_path, minimized]'),
    ("regression_review_hash_ignored", "scripts/regression_intake.py", "test_regression_intake_review.py", 'if review.get("scenario_sha256") != digest:', "if False:"),
    ("regression_digest_only_accepted", "scripts/regression_intake.py", "test_regression_intake_review.py", "if not direct:", "if False:"),
    ("regression_builder_self_reviews", "scripts/regression_intake.py", "test_regression_intake_review.py", 'if review["builder"].strip().casefold() == review["reviewer"].strip().casefold():', "if False:"),
    ("regression_failed_observation_accepted", "scripts/regression_intake.py", "test_regression_intake_review.py", 'observed.get("pass") is not True', "False"),
    ("regression_changed_snapshot_accepted", "scripts/regression_intake.py", "test_regression_intake_review.py", 'read_bytes(snapshot) != candidate_bytes or ', ""),
)


def execute(command, root):
    try:
        return subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired) as error:
        return subprocess.CompletedProcess(command, 127, "", str(error))


def is_assertion_failure(result):
    output = result.stdout + result.stderr
    return (result.returncode == 1
            and re.search(r"Ran [1-9][0-9]* tests?", output) is not None
            and re.search(r"\nFAILED \(failures=[1-9][0-9]*\)\s*\Z", output) is not None)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("output already exists; choose a new evidence file")
    report = {"format_version": 1, "scope": "curated gate, failure-triage and regression-intake mutations; not repository mutation score",
              "source_fingerprint": fingerprint(ROOT), "mutations": [], "passed": False}
    command = [sys.executable, "-B", "-m", "unittest", "discover", "-s", "scripts/tests", "-p", "test_campaign_review.py"]
    with tempfile.TemporaryDirectory(prefix="mtgo-gate-mutants-") as directory:
        root = Path(directory)
        for relative in ("scripts/verification_gate.py", "scripts/verification_evidence.py",
                         "scripts/verification_triage.py", "scripts/tests/test_triage_review.py",
                         "scripts/regression_intake.py", "scripts/tests/test_regression_intake_review.py",
                         "tests/regressions/issue_local_priority_eliminated_player.json",
                         "tests/replays/basic_casting.json",
                         "scripts/tests/test_campaign_review.py", "scripts/tests/fixtures/pr_campaign_review.json",
                         "verification.json"):
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, destination)
        baseline = execute(command, root)
        report["baseline"] = {"exit_code": baseline.returncode, "output": baseline.stdout + baseline.stderr}
        baselines = {"test_campaign_review.py": baseline.returncode == 0 and re.search(r"Ran [1-9][0-9]* tests?", report["baseline"]["output"]) is not None}
        if baseline.returncode == 0 and re.search(r"Ran [1-9][0-9]* tests?", report["baseline"]["output"]):
            for name, relative, pattern, before, after in MUTATIONS:
                source = root / relative
                original = source.read_text()
                selected = [*command[:-1], pattern]
                if pattern not in baselines:
                    control = execute(selected, root)
                    text = control.stdout + control.stderr
                    report.setdefault("additional_baselines", {})[pattern] = {"exit_code": control.returncode, "output": text}
                    baselines[pattern] = control.returncode == 0 and re.search(r"Ran [1-9][0-9]* tests?", text) is not None
                item = {"id": name, "source": relative, "checks": pattern, "before": before, "after": after, "status": "ERROR"}
                if baselines[pattern] and original.count(before) == 1:
                    source.write_text(original.replace(before, after, 1))
                    compile_result = execute([sys.executable, "-B", "-c",
                                              "import sys; from pathlib import Path; compile(Path(sys.argv[1]).read_text(), sys.argv[1], 'exec')",
                                              str(source)], root)
                    item["compile_exit_code"] = compile_result.returncode
                    item["compile_output"] = compile_result.stdout + compile_result.stderr
                    if compile_result.returncode == 0:
                        result = execute(selected, root)
                        item.update(exit_code=result.returncode, output=result.stdout + result.stderr)
                        if result.returncode == 0:
                            item["status"] = "SURVIVED"
                        elif is_assertion_failure(result):
                            item["status"] = "KILLED"
                    source.write_text(original)
                report["mutations"].append(item)
            report["passed"] = bool(report["mutations"]) and len(report["mutations"]) == len(MUTATIONS) and all(m["status"] == "KILLED" for m in report["mutations"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as output:
        json.dump(report, output, indent=2)
        output.write("\n")
    print(json.dumps({"passed": report["passed"], "killed": sum(m["status"] == "KILLED" for m in report["mutations"])}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
