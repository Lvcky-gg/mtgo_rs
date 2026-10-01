# Offer special actions directly from hand cards

Hand card play-option grouping now includes engine-offered SpecialAction entries
for that card. A card whose only legal action is foretell, plot, or suspend is
clickable; a card offering both casting and a special action opens the existing
choice menu rather than silently casting. Special actions for objects outside
the displayed hand are excluded.

The shared grouping helper also serves zone card browsers. Existing descriptive
special-action labels are used in the menu. No rules or legality logic was added.

A regression covers casting plus a special action, an unrelated object, and a
special-action-only card. Code edits are confined to board.rs; engine/compiler
work was preserved. No GUI visual inspection was performed.

Validation: all 20 board tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
