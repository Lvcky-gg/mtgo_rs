# Commander verification increment

Independent rules tests first reproduced the command-zone mismatch: all three
initial assertions failed before production edits. The engine, monarch emblem
creation and session setup now use the existing shared `ZoneRef` for Command.
Normal commander eligibility checks ownership explicitly, so shared membership
does not grant opponents casting permission. Existing engine/session tests now
construct the same valid representation.

Independent golden coverage checks owner-only casting, forged opponent actions,
tax affordability, actual second-cast mana spending and shared return destinations.
A four-player Commander casting replay and permanent representation regression
expand the corpus to seven replays and three regressions. The PR campaign passes
20 observed checks with 10,000 requested cases per generated property and 800
semantic actions. Formatting and all-target/all-feature Clippy pass. Production
mutation smoke kills 3/3 compiled mutants, including removal of the owner check.
The final all-feature workspace run passes 1,831 tests across 72 suites, with
44 ignored tests. Existing network tests ran with the approved local-socket access.

Nine registry primitives remain EXPERIMENTAL. Optional commander return decisions
and broader multiplayer elimination are still unverified; closed-alpha categories
remain unassessed. This increment does not claim full Commander correctness.
