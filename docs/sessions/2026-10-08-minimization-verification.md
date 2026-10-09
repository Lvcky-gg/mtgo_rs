# Failure minimization verification

Independent review reproduced a reducer defect: deleting actions could substitute
an earlier stale checkpoint for the original failing checkpoint because both
reported `First divergent checkpoint`. The reducer now compares a structured
failure signature containing the message, original action content hash when
applicable, and canonical diff paths. Expected checkpoints and assertions are
never rewritten. Actual zone totals are diagnostic rather than assertion identity.

`minimize_with_report` exposes serializable attempt/reduction counts, original
sizes, failure signature and budget exhaustion. The CLI prints a size/budget
summary while retaining the GameScenario output format and existing `minimize`
API. Setup reduction also tries clearing damage, tapped status and individual
counters. With remaining actions, object removal stays trailing-only to avoid
rebinding identities; action-free cases can remove interior objects.

Seven independent reviewer checks cover oracle substitution, byte-equivalent
zero-budget output, metadata preservation, action identity, reducible life
assertions, bounded reports and setup simplification. The deliberate failure
fixture lives in `tests/minimization`, outside the passing engine corpus.
The new compiled mutation restoring message-only comparison is killed by the
independent checkpoint test: 1/1 selected mutants, not a repository mutation
score or a rerun of all 23 curated mutants. Its captured Rust snapshot is
`d2c3122f9025016796a41aced91121d6e3a1635eb3461be0840196375969ee74`;
later reviewer tests and concurrent compiler work are outside that snapshot.

Validation: full workspace tests, formatting, workspace/all-target Clippy with
warnings denied, diff checks, and Python tooling tests pass (18 run, one skipped).
PR campaign passes all 28 checks with eight replays, ten engine regressions and
800 observed semantic actions. The CLI retains all three actions in the deliberate
checkpoint fixture, correctly rejecting four proposed reductions.

Observable failure identity does not prove equivalent Magic causality. Duplicate
identical action checkpoints and changed setup premises still require independent
rules review before regression intake. Player/effect reduction and safe action-ID
remapping remain future work. No confidence promotion or release readiness claim.
