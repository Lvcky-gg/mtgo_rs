# Agent-first verification foundation

Implemented the audited scenario/replay foundation in `mtg-verify`, canonical
full-information snapshots/digests, independent CR/security/property/differential
checks, bounded structural and semantic attack tools, action minimization,
permanent fixture discovery, production mutation smoke, confidence metadata and
PR/nightly/weekly campaign/release policy. Commands and scope are in
[verification.md](../verification.md).

Independent Rules Lawyer, Breaker and attack/trust agents authored verification
separately from the scenario Builder. Canonical review found omitted life state;
a permanent harness regression prevents that false pass. The first full PR
semantic campaign then discovered eliminated-player priority rotation at seed
484. Independent reduction produced two passes with no objects. Production now
skips eliminated seats, synchronizes the priority holder and skips their queued
extra turns; independent golden/acceptance tests and a permanent engine fixture
verify those changes.

Final local results:

* `cargo fmt --all --check`: PASS.
* Workspace all-target/all-feature Clippy with warnings denied: PASS.
* Workspace all-feature tests: 1,827 passed, 44 ignored across 71 suites; existing
  socket tests required the approved unsandboxed test execution.
* PR campaign: 18 checks PASS, six synthetic replays, two regressions, four games
  totaling 800 semantic actions, 10,000 requested cases per generated property.
* Actual production SBA and opponent-hand-leak mutants: 2/2 killed after passing
  unchanged baselines. This is a smoke result, not a repository mutation score.
* Python automation tests: 17 tests, one skipped, PASS.
* Fuzz target compilation: PASS; local cargo-fuzz coverage-guided campaign not run.
* Ordinary verification gate: PASS. Explicit closed-alpha gate: correctly FAILS
  three unassessed categories. All seven seeded confidence primitives remain
  EXPERIMENTAL.

Known scope gaps include Commander command-zone representation, CR 800.4 object/
control-effect cleanup, broader golden/invariant/reference models, real-session
replay capture, complete dependency extraction and automatic failure triage/PRs.
Nightly/weekly runs are configured but not locally measured. Require the GitHub
`Verification / verify` status in branch protection to enforce PR merge policy.
No unrelated card compiler/engine work was discarded or committed.
