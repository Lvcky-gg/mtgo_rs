# Report hand arrivals without assuming they were draws

View-based narration now says "Your hand gained …" for newly visible hand
objects. The view does not distinguish drawing, tutoring, and returning cards,
so the old "You drew" statement could be false. Arrivals are now logged even
when hand size stays constant because another card left during the same update.

Two new regressions cover a hand replacement with unchanged size and a permanent
returned to hand. Existing named-hand-card coverage is retained with neutral
wording. Only the viewer's own visible hand is considered.

Code edits are confined to narrate.rs; engine/compiler work was preserved.

Validation: all 15 narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
