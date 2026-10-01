# Bind lobby identity to the encrypted peer

Added `authenticated_host_handshake` to the network wire layer. Before passing a
Join to the host state machine, it checks that the claimed guest key equals the
identity authenticated by Noise. A mismatch sends a refusal and leaves the invite
unconsumed. The application host uses this entrypoint with `secure.peer()`.
Existing generic `host_handshake` callers retain their API for transports without
an externally authenticated identity.

A real-loopback regression sends a Join claiming a different identity than the
Noise peer, verifies refusal, and then joins successfully with the legitimate
invite. All 115 app library tests pass. Transport-enabled network all-target
Clippy with warnings denied and diff whitespace checks pass. Engine and compiler
work was preserved.

Final validation also passed all 84 transport-enabled network tests, both hosted
network-match integration tests, and app all-target Clippy with dependency lints
disabled and warnings denied.
