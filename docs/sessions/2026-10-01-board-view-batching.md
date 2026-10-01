# Bound board-view processing per frame

The game board now processes at most 128 observed positions per frame and requests
an immediate repaint after a full batch. Remaining positions stay queued rather
than being collected and narrated in one unbounded frame.

Pending questions are read only after earlier observations have drained, so their
newer view cannot be replaced by an older queued observation on the next frame.
A regression queues more than a batch plus a newer question and checks bounded
progress, eventual question delivery, and no board rollback.

Validation: all four UI tests, app all-target Clippy with dependency lints disabled
and warnings denied, and diff checks pass. Changes remain uncommitted.
