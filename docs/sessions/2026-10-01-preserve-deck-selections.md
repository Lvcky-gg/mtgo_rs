# Preserve deck choices across replacement saves

Deck refresh now retains current IDs when present and follows a stored deck's
previous name when a same-name replacement assigns a new ID. This applies to
player, bot, and deck-detail selections. Missing setup choices retain the existing
demo fallback, while missing detail selections are cleared.

Builder save/close and successful deletion refresh from the active Store without
opening another default-path database connection. Saving also invalidates stale
coverage through the shared refresh path.

A regression covers replacement IDs, the demo choice, unknown IDs, renamed rows
with the same ID, and deletion without matching unrelated decks or demo labels.

Validation: all four app event/selection/transfer tests, GUI build, app all-target
Clippy with dependency lints disabled and warnings denied, and diff checks pass.
Changes remain uncommitted.
