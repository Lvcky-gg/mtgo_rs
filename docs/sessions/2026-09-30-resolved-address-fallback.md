# Retry resolved addresses after WebSocket upgrade failure

`WsChannel::connect` now continues to the next resolved socket address when TCP
connects but WebSocket upgrade fails. Previously, only TCP errors advanced to the
next address. The final failure preserves the last TCP or WebSocket error, and
successful connections still retain their buffered messages and clear setup
socket timeouts.

Two deterministic regressions cover a first address returning HTTP 503 followed
by an accepting address, and an empty resolved-address set returning
AddrNotAvailable. Tests use explicit local addresses rather than depending on DNS
ordering. The shared HTTP-request reader also keeps the buffered-message test.

All 89 transport-enabled network tests pass (75 library, seven encryption,
seven socket). Network all-target Clippy with warnings denied and diff whitespace
checks pass. Engine/compiler and app code were left untouched.
Workspace compilation also passes.
