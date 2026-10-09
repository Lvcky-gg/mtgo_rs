# Independent targeting verification

Rules expectations were independently checked against Wizards Comprehensive
Rules 601.2c and 608.2b. New tests in
`crates/mtg-verify/tests/targeting_rules.rs` drive real Engine casting, target
choices, a responding exile spell, priority passes, and stack resolution.

The initial baseline confirmed that an artifact excluded from a creature-only
target choice was nevertheless accepted by Engine::answer. A player target was
also accepted for that creature-only slot. The Builder subsequently corrected
target-answer validation; verification tests were not weakened.

The suite checks illegal announcement targets and malformed answers (wrong
target kind, nonexistent object, missing required targets, excessive count,
surplus target groups), preservation of a rejected pending choice, inability to
cast with no required candidate, all-target illegality suppressing untargeted
life gain, partial illegality allowing the remaining target and untargeted life
gain, and optional zero targets still resolving. The engine asks successive
target specifications separately; the test driver follows that existing API.

Existing replay/regression checkpoints were intentionally regenerated through
`cargo run -p mtg-verify --example seed_corpus` after the Builder's fix. Embedded
card IR, authored expected life/zone assertions, and issue descriptions remain
defined by the seeding tool. Recorded checkpoints are deterministic regression
evidence, not independent rules truth.

Validation: targeting (6), corpus (5), campaign (2), and scenario assertions (3)
tests pass. Targeted Clippy passes. This increment does not establish VERIFIED
confidence for the entire targeting subsystem or networking.

Independent Breaker coverage in `mtg-engine/tests/targeting_review.rs` adds ten
checks for distinct-target assignment, player-controller restrictions changing
before resolution, duplicate rejection, object identity, and target specifications
with mixed cardinalities. The production fix retains each chosen target's
announcement specification for resolution instead of inferring it from its index.

A scoped slow targeting oracle independently enumerates creature candidates for
controller/owner filters, shroud, hexproof, phasing and distinctness. Generated
comparisons run 10,000 cases per property. Unsupported inputs return errors;
this oracle does not claim complete protection or targeting coverage. A saved
proptest seed and `issue_local_phased_out_targeting.json` preserve the discovery.
Scenario initial objects now support the engine's existing `phased_out` flag.

Final validation: workspace tests pass (1,903 passed, 44 ignored across 81 suites),
formatting and workspace Clippy pass, Python harness tests pass (18 tests, one
skipped). The PR campaign passes all 23 checks: eight replays, five regressions,
and 800 observed semantic actions. A fresh compiled-production mutation campaign
kills all 15 curated mutants; the two targeting mutants additionally fail scoped
goldens and generated independent comparisons. These are bounded smoke results,
not repository-wide correctness or mutation coverage.

An older compiled-card test expected automatic filling of a required empty target
selection. The independent reviewer replaced that expectation with rejection,
unchanged prompt, and an explicit legal retry. Optional empty selections continue
to be accepted. No independent test was weakened to accommodate production code.
