# End-of-combat durations

The compiler supports trailing “until end of combat” for creature boosts and
keywords, control changes, and creature animation. Activated abilities can also
say “activate only during combat”, enforced by an Or of DuringStep conditions for
the six combat steps. Leading duration wording remains scoped to its existing
until-end-of-turn parser path.

The engine previously cleared combat and expired UntilEndOfCombat effects as a
turn-based action at the beginning of EndCombat. It now does so after StepEnded
for EndCombat, matching the end of the step after priority. This also ensures an
effect created during that step expires before the second main phase.

Four gameplay regressions verify damage with a temporary boost, keyword expiry,
artifact animation and restoration, combat-only activation availability, control
and haste restoration, and effects created during EndCombat. The new Golem subtype
was appended to the test table without changing existing subtype ids.

Validation: 1,241 engine/Oracle tests passed, 38 ignored. All four focused tests
pass after adding the activation-availability assertion; workspace all-target,
all-feature Clippy and formatting pass. Live coverage: 16,478 / 34,913 playable
as printed (47.2%), up three from the prior snapshot.
