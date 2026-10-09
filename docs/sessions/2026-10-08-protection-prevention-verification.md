# Protection and unpreventable damage verification

An independent Rules Lawyer/Breaker authored nine golden tests in
`crates/mtg-verify/tests/protection_rules.rs`. Before the fix, untargeted red
unpreventable damage was incorrectly suppressed by protection: zero damage,
zero damage events and no lifelink gain instead of three damage and three life.
Two further baseline counterexamples showed skipped shield-counter and Phantom
+1/+1-counter removal. Ordinary prevention, unrelated blue damage, and preserving
finite next-N prevention shields passed their controls.

The Builder moved protection's damage prevention into the existing shared
`prevention::prevent` path, after the unpreventable-damage handling. Combat and
resolution no longer discard protected recipients before that path runs.
Unpreventable damage applies the supported counter-removal side effects while
leaving finite prevention capacity unchanged. Zero damage removes no counters;
a simultaneous batch removes each applicable counter once. Protection's targeting
and blocking restrictions remain separate from damage prevention.

Independent post-fix checks include actual Engine combat, lifelink accounting,
unchanged targeting restrictions, zero damage, and repeated simultaneous calls.
A separate scoped decision-table property runs 10,000 cases by default, respects
scheduled `PROPTEST_CASES` budgets, and compares damage, counter events and finite
capacity conservation. The table excludes competing ordinary prevention effects
and player ordering; it does not establish complete CR616 coverage.

The permanent `issue_local_unpreventable_prevention.json` fixture casts an
untargeted effect against a protected Phantom creature with two shield and two
+1/+1 counters. Authored assertions require life23/20, unchanged battlefield
membership, and exactly one counter of each kind afterward. Scenario zone
assertions now optionally check counter totals over their existing card/owner/zone
selection, using i64 accumulation. Four independently authored harness tests verify
filtering, absent/empty zero, overflow safety, and rejection of incorrect counter
totals during both replay and recording. Recording cannot bless those incorrect
rules expectations.

Primary source: [Wizards Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt)
702.16e, 615.12, 122.1c and 702.15b.
The independent reviewer read the official June19 PDF when the September TXT
fetch failed. The Builder also checked the September25 TXT through the cached
browser source and confirmed these propositions.

Validation: 2,007 workspace tests pass, 44 ignored across 87 suites; formatting,
workspace Clippy, diff checks and Python harness tests pass (18 tests, one skipped).
PR campaign passes 28 checks: eight replays, ten regressions and 800 observed
semantic actions. The fresh production campaign kills 22/22 curated mutants.
All three new mutants fail independent goldens, the generated table, and the
human-asserted regression corpus; the protection mutant also fails actual combat.

Mutation evidence is tied to Rust snapshot
`10205783fca5139ec33392ee53a475156a6c9ffa05a45470a3157e3b9650dcd1`
(`/tmp/mtgo-prevention-mutants.json`). Concurrent compiler work changed the wider
working-tree fingerprint afterward; this report does not certify later revisions.
No primitive or card receives confidence promotion; all registry entries remain
EXPERIMENTAL. Broader prevention ordering, source last-known information, player
protection, replacement interactions and further additional effects need coverage.
