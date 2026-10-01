# Observe cancellation during guest setup

The UI now passes its existing cancellation flag to `net::join_with_cancel`.
Join setup checks cancellation before parsing, between endpoint attempts, after
WebSocket setup, after encrypted authentication, and after lobby negotiation.
Leaving while joining no longer starts new endpoint attempts or enters match play
once the current operation returns. Existing `net::join` callers retain the
original API through a wrapper without cancellation.

Cancellation is cooperative: current socket operations finish or time out first,
and system DNS lookup still has no independent deadline. Cancellation during a
lobby handshake can consume an accepted invite; that handshake is not retried.

Two regression tests cover cancellation before any work and cancellation during
a failed connection before trying the next address. All 114 app library tests
pass. App all-target Clippy with dependency lints disabled and warnings denied
passes. Diff whitespace checks pass. Engine/compiler work was preserved.

Both network-match integration tests pass, including a hosted match joined by
invite and played to completion by bots.
