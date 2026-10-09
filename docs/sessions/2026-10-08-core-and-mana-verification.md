# Core actions and independent mana verification

Added nine independently authored core-action golden propositions, covering draw,
discard, mill, life/damage semantics, failed-draw timing, zero amounts and zone
identity/ownership. Draw is explicitly distinguished from merely moving cards
into hand. Direct fixture zone-count assertions use existing engine types and
are enforced during recording as well as replay; three independent acceptance
tests prevent recording from blessing incorrect expectations.

The independent scoped mana oracle enumerates concrete source-color assignments,
while production uses bipartite matching. Ten thousand generated cases pass,
including independent resource-conservation/payment-witness checks. Unsupported
reference inputs are rejected. The actual production generic-payment boundary
mutant is killed by explicit and generated tests.

An independent multiplayer reviewer supplied seven acceptance tests against the
concurrent Builder's leave-game work. They caught a real source-owner ability
removal bug, subsequently fixed by the Builder. Fixtures assert their layer-control
preconditions. No test counts promote primitives to VERIFIED.

Final workspace validation passes 1,872 tests with 44 ignored across 77 suites.
The 22-check PR campaign passes, including all eight replays/four regressions,
800 semantic actions and 10,000 requested cases per property. Workspace formatting
and all-target/all-feature Clippy pass. A fresh-target production campaign kills
11/11 compiled mutants after passing unchanged baselines.

A shared mutation build cache initially retained the prior mana mutant and caused
a baseline failure. It correctly produced ERROR rather than a kill. Copied sources
now get fresh mtimes, and each campaign receives its own disposable target
namespace. Independent overlapping-run tests check isolation. The updated harness
also passes a real scoped mana mutation run with unique target isolation.

All eleven registry primitives remain EXPERIMENTAL. Closed-alpha assessments
remain unassessed, and broader rules support is still incomplete. The mana model
is deliberately scoped, not a full Magic cost oracle.
