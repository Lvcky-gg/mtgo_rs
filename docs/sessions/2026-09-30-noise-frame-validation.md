# Encrypted frame validation

Continued the network-limit handoff in `mtg-net/src/noise.rs`.

Chunk headers must now be exactly four bytes; trailing bytes previously passed
silently. Encrypted transport messages must contain 16–65535 bytes, so malformed
sizes fail before allocating the decryption buffer, including on non-WebSocket
channels.

Four regression tests exercise authenticated malformed headers and chunk counts,
invalid ciphertext sizes, rejection of sends over 32 MiB without corrupting the
next frame, and a hostile authenticated peer exceeding the reassembly limit with
larger-than-normal encrypted chunks.

Validation: 80 network tests passed with `--features transport` (68 library,
7 encryption integration, 5 socket integration). Loopback tests require execution
outside the filesystem/network sandbox. Transport-enabled all-target Clippy with
warnings denied, workspace compilation, and `git diff --check` passed.

Existing rules-engine/compiler changes were left untouched.
