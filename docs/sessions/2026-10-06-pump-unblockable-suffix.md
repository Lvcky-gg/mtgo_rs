# Pump and unblockable suffix

The until-end-of-turn pump/keyword parser accepts trailing “and can't be blocked
this turn,” appending the existing CantBeBlockedExceptBy(Not Any) restriction
to the same effect with UntilEndOfTurn duration. No engine/IR changes.

Frozen comparison: 16,154 → 16,175 / 34,913 (46.3%), 21 cards / 22 faces gained,
zero lost. Includes You Come to a River, Distortion Strike, Teleportal,
Elusive Spellfist, Pym Particles, Ant-Man, Reformed Rogue, Temmet,
Sea Hag/Aquatic Ingress. Baseline includes concurrent Claude changes.
Snapshot /tmp/mtgo-pump-unblockable-comparison; outputs
/tmp/mtgo-pump-unblockable-before.txt and /tmp/mtgo-pump-unblockable-after.txt.

Parameterized unblockable_pump.rs gameplay test covers power bonus and haste
variants, target-only application, and combat blocking validation rejecting
blocks of the affected creature while permitting blocks of another creature.
Oracle 878 passed, 15 existing ignored. All-target Oracle Clippy passed.
