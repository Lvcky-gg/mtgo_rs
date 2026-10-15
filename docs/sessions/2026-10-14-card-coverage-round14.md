# Card coverage, round 14 — 2026-10-14

Coverage rises from 17,871 to 17,878 / 34,914 (51.2%) using the same
`/tmp/card-coverage.sqlite` snapshot. Complete card-name comparison gains seven
cards and loses none. The existing five-card name-lookup difference remains.

Phasing is a supported keyword. At untap, incoming and outgoing sets are both
determined before applying either, so an object phasing in does not immediately
phase out again. The permanent retains its identity, counters, and tapped status;
one that phases in can then untap normally.

Direct phasing records the controller under whom the permanent phased out.
It returns during that player's untap even when temporary control has since
ended. Attached permanents phase indirectly with their host, transitively, and
return with it rather than during their own controller's untap. Indirect phasing
takes precedence when an attachment would also phase out directly.

Phasing out removes a permanent from combat. Ordinary battlefield and state
triggers of phased-out permanents are suppressed. The phasing metadata is
cleared on phasing in or changing zones, and optional serialized fields preserve
the ability to read older events without that metadata.

Newly playable: Breezekeeper; Merfolk Raiders; Sandbar Crocodile; Taniwha;
Teferi's Drake; Teferi's Isle; Tolarian Drake.

Five gameplay regressions cover alternating untaps, preserved identity and
counters, untapping after return, attachments controlled by another player,
temporary control expiring while phased out, absent observers, and Taniwha's
upkeep trigger only functioning on the turns it is phased in.

Validation: targeted compiled tests; `cargo test --workspace --quiet`;
`cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all -- --check`; and `git diff --check` pass. Workspace tests run
outside the sandbox for their existing loopback networking tests. No existing
fixture was updated. Changes remain uncommitted.
