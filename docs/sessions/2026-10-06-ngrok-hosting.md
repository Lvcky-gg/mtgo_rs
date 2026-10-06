# Automatic local discovery and ngrok hosting

Hosts can choose Nearby players or Friends online. LAN lobbies advertise a
signed, expiring invite through mDNS, and the join screen lists nearby games.
Online hosting embeds the ngrok Rust SDK and opens an HTTPS endpoint pointing
to the game's ephemeral local listener. The host pastes an authtoken; guests
need no account, token, executable, IP address, or router configuration.

Tunnel endpoints have their own signed invite tag, distinct from future relay
endpoints. WebSocket clients preserve paths and queries, validate WSS
certificates against public roots, and include ngrok's programmatic-client
header. Noise continues to authenticate the invited host and encrypt all game
messages. Existing invite expiry and single-use behavior are preserved.

Tunnel startup is cancellable and bounded to 30 seconds. The owner retains the
SDK session and forwarding runtime for the whole hosted match, then closes both
on completion, cancellation, or error. A cancellation watcher shuts down the
active socket so leaving also interrupts a blocked peer read. Provider failures
are summarized without logging credentials. Optional remembered tokens use the game's local database
and are not encrypted at rest; remembering is off by default.

The ngrok SDK currently requires aws-lc compiled in even with its ring feature,
while existing card downloads enable ring. Select a process TLS provider before
SDK startup; the WebSocket client explicitly chooses ring for its TLS connector.

User instructions: [Play with friends](../hosting.md).

Validation covers signed discovery records, expired/misnamed advertisements,
URL routing and ngrok headers, TLS failure without plaintext downgrade, invited
host authentication, impostor rejection and fallback, cancellable provider
startup, and a complete bot match over the existing network hosting path. Live
ngrok validation requires an account token and is an explicit ignored test.

A separate self-hosted relay is deferred in favor of the requested embedded
tunnel. LAN and internet hosting use the same match protocol.

Checks:

```sh
cargo test -p mtg-net --features transport -p mtg-app --offline --quiet
cargo test -p mtg-net --features transport a_lobby_is_discovered_and_removed_over_mdns --offline -- --ignored
cargo clippy -p mtg-app -p mtg-net --features mtg-net/transport --all-targets --offline -- -D warnings
cargo fmt --check -p mtg-app -p mtg-net
cargo build -p mtg-app --bin mtg-gui --offline
```

The real multicast test found and removed a lobby. Inspected the native online
hosting form at 847 pixels wide: token masking, setup links, remembering/forgetting,
advanced settings, and the Host control are visible. No live ngrok endpoint was
opened because an account token was not supplied for validation. Developers can
set `NGROK_AUTHTOKEN` (and optionally `NGROK_DOMAIN`) and explicitly run:

```sh
cargo test -p mtg-app a_live_ngrok_tunnel_plays_a_complete_match -- --ignored
```

That test advertises only the public tunnel, preventing LAN fallback from hiding
an internet-connection failure.

Final results: 333 tests passed in the app/net suite (6 ignored), plus the explicit
multicast test passed. Clippy with warnings denied, formatting, and the GUI build
passed. The final suite includes assigned-domain validation and cancellation
while an authenticated peer is silent.
