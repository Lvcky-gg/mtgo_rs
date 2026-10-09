# Reproducible automatic-transition failures

Advance-budget exhaustion formerly returned an error from scenario replay.
Semantic campaigns could report the failure and emit its input, but capture and
minimization could not process that reproduction. Initial invariant failures
also incorrectly identified action 0 even when no answer had been applied.

Replay now emits structured FAIL evidence for exhaustion, preserving actual
canonical stopped state and digest. Initial failures have no divergent action;
post-answer exhaustion identifies the triggering action. Recording still returns
FAIL and cannot establish a passing baseline from interrupted execution.

GameScenario records a positive per-boundary `advance_budget`, capped at 10,000.
Existing fixtures default to 10,000 and omit that field when serialized. Replay,
semantic generation, capture and minimization retain the same budget. The existing
`next_choice` API retains its default; `next_choice_with_budget` supports explicit
bounded execution. Semantic reports now expose their observed RunReport, and the
fuzz CLI prints it after saving the reproduction. Generated-answer rejection
messages use the same wording as replay so observations compare directly.

Seven independently authored tests cover initial and post-action exhaustion,
capture/roundtrip replay, failed recording, minimization, semantic/report parity,
successful observations, legacy defaults, invalid budgets and the complete CLI
export/reproduction flow. A creature-resolution/enters-trigger replay with budget 4
exhausts at action 2. The minimal permanent deliberately failing harness fixture
`tests/failures/advance_budget_exhaustion.json` is outside the passing engine corpus.
These small budgets deliberately interrupt correct legal transitions; neither test
is a claim that the engine loops infinitely.

Validation: 2,039 workspace tests passed, 44 ignored; formatting, workspace/all-target
Clippy with warnings denied, diff checks, and Python 18 tests (one skipped) pass.
PR campaign passes all 29 checks with 8 replays, 11 engine regressions and 800
observed semantic actions; the deliberate failure fixture runs through its
independent harness tests rather than that passing corpus.
Four selected compiled production mutations are killed: unstructured initial
exhaustion, ignoring the stored budget, inventing an initial divergent action, and
the previously existing zero-action normalization mutation with its updated source
anchor. Captured Rust snapshot:
`cc6370bba81cb1bb7a593a3edcddcb9b6dd7fcd43b813ef619a3882123694206`.
There are 29 configured curated mutations; the other 25 were not rerun. This is
snapshot-scoped harness evidence, not a repository mutation score.

The budget counts automatic `Engine::advance` calls and does not interrupt a
hang inside one call. Process-level campaign timeouts remain necessary. Parse/setup
errors and panics are not yet structured replay attachments. Exhaustion needs
review before being classified as nontermination, and minimization still needs
causal/rules review. No confidence promotion or release-readiness claim.
