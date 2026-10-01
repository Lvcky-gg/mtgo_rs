# Preserve saved decks when renamed writes fail

The builder now saves through Store::put_deck_replacing, which deletes the old
saved ID and writes the replacement in one SQLite transaction. Previously a
rename deleted the old deck in a separate operation before the replacement could
fail. Existing same-name replacement behavior is preserved.

A regression injects a SQLite insert failure after replacement starts and checks
that the original saved deck remains intact, the draft keeps its edited name and
dirty flag, and retrying after the failure saves the renamed deck successfully.

Validation: all 29 builder tests, all 17 store tests, app/store all-target Clippy
with dependency lints disabled and warnings denied pass. Changes remain
uncommitted.
