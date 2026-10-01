# Report public changes of control

The game log now reports when a persistent battlefield object changes controller,
naming the new controller and the currently visible card. It compares object IDs
present on the battlefield in both views, so control changes do not create false
battlefield-entry messages. Hidden identities retain the normal hidden fallback.

A regression covers gaining and losing control between viewer and opponent and
requires exactly the control-change message in each direction. Code edits are
confined to narrate.rs; engine/compiler work was preserved.

Validation: all 12 narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
