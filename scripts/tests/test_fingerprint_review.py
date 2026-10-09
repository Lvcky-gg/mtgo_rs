"""Independent source binding: executable inputs count, generated evidence does not."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "verification_evidence.py"
spec = importlib.util.spec_from_file_location("fingerprint_review", SCRIPT)
evidence = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evidence)

FILES = {
    "Cargo.toml": "manifest", "Cargo.lock": "lock",
    "crates/mtg-engine/src/lib.rs": "engine", "crates/mtg-verify/tests/review.rs": "test",
    "scripts/tests/fixtures/report.json": "static test input",
    ".cargo/config.toml": "build config", "rust-toolchain.toml": "toolchain",
    "tests/replays/game.json": "replay", "tests/regressions/bug.json": "regression",
    "tests/failures/bounded.json": "failure oracle", "tests/minimization/input.json": "reduction oracle",
    "verification.json": "known issue release gate policy",
    "verification/primitives.json": "primitive confidence thresholds and dependencies",
}

def populate(root, reverse=False):
    items = list(FILES.items())
    for name, content in reversed(items) if reverse else items:
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)


class FingerprintReview(unittest.TestCase):
    def test_each_engine_test_data_config_and_corpus_input_changes_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            populate(root)
            baseline = evidence.fingerprint(root)
            for name, content in FILES.items():
                with self.subTest(path=name):
                    (root / name).write_text(content + " changed")
                    self.assertNotEqual(evidence.fingerprint(root), baseline)
                    (root / name).write_text(content)
                    self.assertEqual(evidence.fingerprint(root), baseline)

    def test_generated_logs_and_target_outputs_do_not_invalidate_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            populate(root)
            baseline = evidence.fingerprint(root)
            for name in ["target/debug/generated.rs", "target/debug/output.json",
                         "verification-artifacts/format.log", "verification-artifacts/report.json"]:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("generated evidence")
            self.assertEqual(evidence.fingerprint(root), baseline)

    def test_absolute_root_and_discovery_creation_order_do_not_change_digest(self):
        with tempfile.TemporaryDirectory() as first, tempfile.TemporaryDirectory() as second:
            populate(Path(first))
            populate(Path(second), reverse=True)
            self.assertEqual(evidence.fingerprint(first), evidence.fingerprint(second))
