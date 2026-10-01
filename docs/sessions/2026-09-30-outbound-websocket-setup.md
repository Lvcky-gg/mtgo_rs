# Outbound WebSocket setup

`WsChannel::connect` now resolves the endpoint and uses a ten-second TCP connect
 timeout per resolved address, followed by ten-second read/write timeouts for the
WebSocket upgrade. Successful connections clear upgrade timeouts. DNS remains a
blocking system-resolver operation without a separate deadline. Connections now
use the direct endpoint; HTTP redirects are no longer followed.

The client preserves the original upgraded WebSocket rather than extracting its
TCP stream and building a new WebSocket. This keeps frames buffered alongside
the HTTP response. Bare and bracketed IPv6 hosts are normalized for resolution
and URI construction.

Interrupted blocking upgrades now report a timeout I/O error in both directions.

Two new tests cover a silent upgrade server and an HTTP upgrade response bundled
with the first application frame. All 84 transport-enabled network tests pass
(70 library, seven encryption integration, seven socket integration). Network
all-target Clippy with warnings denied, workspace compilation, and diff whitespace
checks pass. Engine/compiler work was preserved.
