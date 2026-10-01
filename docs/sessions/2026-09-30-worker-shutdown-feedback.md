# Report unexpected match worker shutdown

`drain_events` now distinguishes an empty event queue from a disconnected one.
If the worker disconnects without a final result, the UI records an explicit
unexpected-worker-stop error instead of continuing to display a waiting spinner.
An already-delivered match result is preserved. Both normal completion and
unexpected shutdown clear any stale sideboarding dialog.

Two regression tests cover shutdown without a final event, and a final event
queued behind more than one UI batch. The latter also verifies the retained
status cap and latest-message ordering from the earlier UI change. Both tests
pass. App all-target Clippy with dependency lints disabled and warnings denied,
and diff whitespace checks pass. No GUI visual inspection was performed.

Code edits are confined to `mtg-app/src/app.rs`; concurrent engine/compiler work
was left untouched.
