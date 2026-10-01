# Preserve the deck snapshot on read failures

Deck listing now returns database errors. App refresh reads the card count and
deck list before replacing displayed state; failure preserves the last successful
snapshot and deck choices. The database error banner now supports refresh errors
as well as opening errors. Retry refreshes the active connection when available,
or retries opening on the next frame otherwise.

A regression induces a deck-table read failure, verifies it is reported rather
than returned as a demo-only list, restores the table, and checks recovery.

Validation: all 6 deck tests, app library/binary Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Changed source formatted.
Changes remain uncommitted; other workers' files were preserved.
