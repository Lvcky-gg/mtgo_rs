# Bound peer WebSocket handshakes

`WsChannel::accept` now gives the WebSocket upgrade ten seconds to complete and
then returns a connection error. It clears the temporary read/write timeouts once
the peer is accepted, so normal match traffic remains blocking as before. This
keeps a peer that opens TCP and stalls from holding the host's accept loop forever.

Both sides now cap each WebSocket message at 64 KiB, matching the encrypted
Noise chunk size with room for its tag. The Noise channel also caps a complete
reassembled frame at 32 MiB and checks cumulative bytes before extending its
buffer, preventing a peer from forcing a large frame allocation.

Validation: `cargo check --workspace`, `cargo clippy -p mtg-net --all-targets -- -D
warnings`, and `git diff --check` passed. No tests were run.
