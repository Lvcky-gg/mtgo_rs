# Preserve empty deck rows on one-copy removal

Removing one copy now requires a positive count. A saved zero-count main or
sideboard row previously underflowed, panicking in debug or becoming u32::MAX
in release. Rejected removal leaves contents, dirty state, and undo/redo history
unchanged.

A regression loads zero-count rows from the real in-memory store, creates a
redo operation, removes from both empty rows, and verifies that redo still works.

Validation: all 35 builder tests, app library/binary Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Source formatted directly.
Changes remain uncommitted.
