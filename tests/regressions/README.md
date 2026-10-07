Every confirmed engine bug must acquire a minimized permanent JSON scenario
before being considered fixed. Name fixtures `issue_0481_clone_blood_moon.json`
and fill metadata issue, description, rules, first_affected (if known), and
fixed_in. The shared corpus test automatically discovers every JSON here.

The initial fixture `issue_local_canonical_life_omission.json` records a genuine
verification-subsystem bug: player life was omitted from the diagnostic state,
so distinct life totals initially produced identical canonical digests. Its
companion corpus test changes life and requires divergence after the first
answer, without relying on a separate explicit life assertion.

This is a harness regression, not an invented historical Magic-engine issue.
Harness and rules-engine regressions must be distinguished in reports.
Regression count must not decrease without an explicit rules-version migration.

`issue_local_priority_eliminated_player.json` is a confirmed engine regression
discovered by semantic campaign seed 484, then independently reduced to two
passes and no zone objects. Its golden companion asserts that the next surviving
player receives both the priority choice and the rules-state priority holder.

`issue_local_shared_command_zone_casting.json` records the confirmed engine
representation mismatch between shared Command zones and player-keyed casting
lookup. Independent golden tests also cover owner-only permission, the tax
affordability boundary, and shared command-zone return destinations.

`issue_local_forced_commander_return.json` records the confirmed suppression of
the owner's optional CR903.9a decision. Its minimal fixture has one lethal
commander and one owner-declines answer; independent golden tests assert the
declined card remains in the graveyard and is not repeatedly offered.
