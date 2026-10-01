# Distinguish alternative casting options

The card-text snapshot now records alternative costs by engine ability ID. The
UI names dash, evoke, overload, warp, and bestow and shows the associated mana
cost, replacing identical generic "alternative cost" buttons. Hidden or unknown
cards retain a generic identity-safe fallback. Labels follow the visible face.

A regression covers all five mechanics with sparse ability IDs and distinct
costs, plus hidden identity fallback. Code edits are in cards_text.rs and ui.rs;
engine/compiler work was preserved. No GUI visual inspection was performed.

Validation: all nine card-text tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
