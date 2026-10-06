# Fetch-land visibility fix

The user reported that fetched lands do not appear on the battlefield after
tutoring. A compiled-card regression test using a tap/pay-life/sacrifice fetch
ability succeeds: the chosen land enters untapped under the activating player's
control, and the source is sacrificed and life paid. The engine's search and
movement path therefore worked in this reproduction.

Reproduced a client display failure with six lands followed by a newly fetched
land in a narrow board. The battlefield used `horizontal_wrapped` around
composite card widgets. Those widgets did not wrap: the last card was outside the
board's clip rectangle, and no text shape was painted for it. New objects sort
after existing ones, which made newly fetched lands particularly likely to be
hidden.

Changed `crates/mtg-app/src/ui.rs` to lay out battlefield cards in explicit grid
rows sized to the available width. Tapped artwork dimensions are included when
calculating the column count. The arena also tracks a newly added land belonging
to the viewer and scrolls its card into view once it is rendered. A previously
hidden local land can therefore be seen without scrolling manually.

Added two renderer regression tests: the last land stays within the board width,
and a newly fetched land appears inside the visible board after layout and scroll
settle. Added a compiled fetch activation regression in
`crates/mtg-oracle/tests/compiled/search.rs`, and a before/after fetch fixture to
`crates/mtg-app/examples/visual_states.rs`.

Verified native capture at 847×689:
`/tmp/mtgo-fetch-visual/10-fetched-land.png`. It shows the seventh land in its own
grid row, with the journal confirming its battlefield entry.

Validation:

```sh
cargo test -p mtg-app -p mtg-oracle --offline
cargo clippy -p mtg-app --lib --example visual_states --offline -- -D warnings
cargo build -p mtg-app --bin mtg-gui --example visual_states --offline
```

896 tests passed, zero failed, 19 ignored. Clippy, build, and whitespace checks
passed. Test output: `/tmp/mtgo-fetch-tests.log`. The GUI executable was rebuilt;
an already running client must be restarted to use the fix. Concurrent existing
rules/compiler changes were preserved.
