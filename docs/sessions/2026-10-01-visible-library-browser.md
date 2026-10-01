# Browse disclosed library cards

Added a "top card" button beside each player's library count when their library
has cards present in PlayerView.visible. It opens a browser of only those
disclosed objects, grouped by ZoneRef.player rather than controller. Hidden
library contents are never fetched or inferred.

Library cards use the existing card browser and engine-provided play options.
Zone card browsers and the play-choice menu now preserve alternative casting
options through the same grouping helper used by the hand. The menu label is
"Choose how to play this card" to cover faces and alternative costs.

A regression covers an undisclosed library and disclosed cards whose controller
differs from their library's player. Code edits are in board.rs and ui.rs;
engine/compiler work was preserved. No GUI visual inspection was performed.

Validation: all 19 board tests pass; app compilation, app all-target Clippy with
dependency lints disabled and warnings denied, and diff checks pass.
