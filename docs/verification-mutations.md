# Production mutation smoke verification

Run `python3 scripts/verify_mutations.py --output target/verification/mutations.json`.
The script copies the current workspace sources into a disposable temporary
workspace. Production files in the shared working tree are never mutated.
A separate Cargo target directory avoids mixing baseline workspace artifacts.

Each test must first pass on the unchanged baseline. Every production mutant must
compile. Compilation failures, timeouts, zero tests, and infrastructure failures
are errors rather than mutation kills. The JSON report includes exact commands,
test logs, counterexamples, mutation anchors, and the copied Rust-source digest.
The temporary workspace is removed on exit.

The campaign deliberately targets four defects:

| Production mutation | Independent detection mechanisms |
| --- | --- |
| CR 704.5f toughness `<= 0` becomes `< 0` | Rules golden test, generated property, independent decision-table differential |
| Opponent hand identities exposed by `PlayerView` | Hidden-information noninterference property |
| Shared command-zone casting ignores card ownership | Independently authored owner-only Commander golden test |
| Commander return confirmation acceptance is inverted | Independently authored graveyard/exile accept-and-decline golden test |

The local campaign killed all four compiled mutants. All three SBA mechanisms
failed independently, the hand-leak property failed, and the Commander ownership
golden tests failed. This is a **four-mutant smoke result**, not a claim about
repository-wide mutation coverage or a VERIFIED primitive. Controlled predicate demonstrations in `mutation.rs` are also kept
separate from production mutation evidence.

Confidence metadata requires independently reviewed, same-build artifacts and a
larger production mutation sample. CI should archive the JSON report. Extending
this campaign should add a concrete production mutation and an independently
specified behavioral check; changing tests to follow the mutated implementation
invalidates the evidence.
