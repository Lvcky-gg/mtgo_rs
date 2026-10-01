# Bound encrypted identity and lobby setup

Host and guest now set ten-second socket read/write timeouts before the Noise
identity exchange and retain them through the invite/seed handshake. They clear
these timeouts before entering match play, where player choices may take longer.
This is a per-I/O timeout, not an overall handshake deadline.

`WsChannel::set_io_timeout` configures both socket directions, and
`NoiseChannel::inner_mut` permits transport configuration after encryption setup.

Two real-loopback regression tests cover a silent peer timing out during Noise
setup and a delayed answer surviving after the setup timeout is cleared. All
seven socket integration tests pass. Workspace compilation passed after the
initial edits. A subsequent app Clippy run was blocked by concurrent engine work:
`engine.rs` references the not-yet-present `mtg_ir::walk::time_counter` function.
No engine files were changed for this task.
