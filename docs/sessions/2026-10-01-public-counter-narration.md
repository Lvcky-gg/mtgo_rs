# Narrate energy and poison changes

The view-based game log now reports energy and poison gains/losses for each
player, including the resulting total. Wording follows the existing viewer and
opponent conventions. Counter deltas use i64 conversions so unsigned decreases do
not underflow. Unchanged totals produce no message.

A regression covers viewer energy loss/gain, opponent poison gain/loss, and no
change. Narration uses only existing public PlayerSummary data. Code edits are
confined to narrate.rs; engine/compiler work was preserved.

Validation: all nine narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
