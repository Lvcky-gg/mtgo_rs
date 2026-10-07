# Structural fuzzing

Run the shared bounded smoke suite without nightly tooling:

```sh
PROPTEST_CASES=10000 cargo test -p mtg-verify --test structural
```

With cargo-fuzz installed and a nightly Rust toolchain:

```sh
cargo +nightly fuzz run structural -- -max_len=65536 -timeout=5 -rss_limit_mb=2048 -max_total_time=60
```

The isolated fuzz workspace uses the same `mtg_verify::structural::exercise`
entry point as CI smoke tests. Inputs over 64KiB are rejected before parsing;
serde_json recursion limits remain enabled. The libFuzzer process limits protect
campaigns from slow inputs and excessive allocations; the input bound alone is
not a proof of bounded runtime or memory.

Real boundaries exercised: scenario/replay JSON validation, CardFace IR JSON,
deck text, card-text compiler, GuestMessage/HostMessage JSON, and external Scryfall
card deserialization/conversion. Successful protocol deserialization is not
semantic protocol acceptance. This does not cover encrypted framing, sockets,
SQLite save files, or a complete external bulk-import transaction.

Commit minimized libFuzzer findings as permanent regression fixtures before
considering a defect fixed. Store fuzz seed/corpus, crashing bytes, source/build
identifier, and failure output. Do not expose scenario artifacts containing
private game information to opponents.
