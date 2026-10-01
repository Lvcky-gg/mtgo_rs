# Bound database progress processing per frame

The database updater processes at most 128 queued progress messages per UI
frame. A full batch requests an immediate repaint; a waiting worker retains the
existing periodic repaint. Completion and unexpected worker shutdown remain
separate outcomes, and a queued completion behind a backlog is preserved even
when its sender has disconnected.

Regressions verify bounded draining with a final result behind the batch and
separate waiting versus shutdown behavior.

Validation: all 7 app event tests, app library/binary Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Source formatted directly.
Changes remain uncommitted.
