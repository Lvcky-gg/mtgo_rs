# Show public energy counters

The board arrangement now retains PlayerSummary.energy, and each player's header
displays a nonzero energy total beside life and poison. Zero totals stay omitted.
This uses existing public view data without changing engine state or legality.

Extended the existing player-total regression to cover different energy totals
for the viewer and opponent. Code edits are in board.rs and ui.rs; concurrent
engine/compiler work was preserved. No GUI visual inspection was performed.

Validation: all 19 board tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
