# Preserve full deck rows during edits

Adding to a row at u32::MAX or moving a copy into that row now returns before
changing counts or recording undo history. A move previously decremented its
source before the destination increment overflowed. Both main and sideboard
rows use the same guard.

The regression checks both directions, verifies rejected edits preserve counts
and history, then removes a copy and verifies a subsequent move succeeds and
both accepted edits can be undone.

Validation: all 32 builder tests pass. Source formatted directly. App library
and binary Clippy with dependency lints disabled and warnings denied passes;
diff checks pass. Changes remain uncommitted.
