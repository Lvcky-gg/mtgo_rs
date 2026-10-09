"""Bind observed campaigns to executable sources and the complete fixture corpus."""
import hashlib
from pathlib import Path

MAX_SEMANTIC_GAMES = 100_000


def fingerprint(root):
    root = Path(root)
    paths = {root / "Cargo.toml", root / "Cargo.lock", root / "verification.json"}
    paths.update((root / "verification").rglob("*.json"))
    paths.update(root / name for name in ("rust-toolchain", "rust-toolchain.toml", ".cargo/config", ".cargo/config.toml"))
    paths.update((root / "crates").rglob("*.rs"))
    paths.update((root / "crates").rglob("Cargo.toml"))
    paths.update((root / "crates").rglob("*.proptest-regressions"))
    paths.update((root / "scripts").glob("*.py"))
    paths.update((root / "scripts/tests").glob("*.py"))
    paths.update((root / "scripts/tests/fixtures").rglob("*.json"))
    paths.update((root / ".github/workflows").glob("*.yml"))
    for corpus in ("replays", "regressions", "failures", "minimization"):
        paths.update((root / "tests" / corpus).rglob("*.json"))
    paths.update((root / "tests/regressions").rglob("*.review"))
    digest = hashlib.sha256()
    for path in sorted(paths):
        if path.is_file():
            digest.update(path.relative_to(root).as_posix().encode())
            digest.update(b"\0")
            data = path.read_bytes()
            digest.update(len(data).to_bytes(8, "big"))
            digest.update(data)
    return digest.hexdigest()


def planned_checks(corpora, games, tier):
    if tier not in ("pr", "nightly", "weekly"):
        raise ValueError("unsupported campaign tier")
    if type(games) is not int or not 1 <= games <= MAX_SEMANTIC_GAMES:
        raise ValueError("invalid planned semantic game count")
    if not isinstance(corpora, dict) or set(corpora) != {"replays", "regressions"}:
        raise ValueError("missing planned corpus inventory")
    checks = ["verification_tests", "build_replay_cli"]
    for corpus in ("replays", "regressions"):
        fixtures = corpora[corpus]
        if (not isinstance(fixtures, list) or not fixtures
                or any(not isinstance(p, str) or not p.startswith(f"tests/{corpus}/")
                       or ".." in Path(p).parts or not p.endswith(".json") for p in fixtures)
                or len(set(fixtures)) != len(fixtures)):
            raise ValueError("invalid planned corpus: " + corpus)
        checks.extend(f"{corpus}_{index}" for index in range(len(fixtures)))
    for index in range(games):
        checks.extend((f"semantic_{index}", f"semantic_replay_{index}"))
    if tier == "weekly":
        checks.extend(("production_mutations", "gate_mutations"))
    return checks


def plan(root, games, tier):
    root = Path(root)
    corpora = {name: [p.relative_to(root).as_posix()
                     for p in sorted((root / "tests" / name).rglob("*.json"))]
               for name in ("replays", "regressions")}
    return {"corpora": corpora, "semantic_games": games,
            "checks": planned_checks(corpora, games, tier)}
