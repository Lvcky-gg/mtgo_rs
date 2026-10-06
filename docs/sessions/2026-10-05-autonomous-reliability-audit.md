# Autonomous reliability audit

Worked through reproduced defects in bot combat, deck import, storage, identity
startup, and the headless driver. Existing UI edits and concurrent engine/compiler
implementation changes were preserved. No commit or manual desktop game was made.

## Bot combat

`mtg-policy/src/bot/{mod,attack_strategy,block_strategy}.rs` uses i64 for aggregate
damage, blocked damage estimates, and loyalty pressure. Counter changes and marked
u32 damage are combined before clamping effective stats. Discard ranking uses
`Reverse` instead of subtracting mana values from u32::MAX. Split mana values
saturate instead of wrapping. First/double strike fights account for both damage
steps and for creatures dying during the first step. Attack selection now counts
double strike when deciding whether lethal damage justifies losing attackers.

Nine new tests cover arithmetic boundaries, signed damage conversion, discard
ranking, double strike fights, safe blocks, and lethal attacks. The six arithmetic
regressions and the double strike regressions failed before their fixes. All 40
policy tests pass.

## Deck import and CLI

`mtg-deck/src/{parse,resolve}.rs` requires whitespace between leading quantities
and names, accepts whitespace before trailing quantities, uses u64 section
totals, drops typographic quotes consistently with plain punctuation, and ignores
zero-copy entries during resolution. Explicit headers anywhere in a list disable
the implicit blank-line sideboard convention. This prevents cards before a later
Sideboard header from silently moving out of the main deck. Six new regressions
pass; all 24 deck tests pass.

`mtg-headless/src/bin/cards.rs` rejects companion designation and multiple
commanders before saving, trims deck names, and preserves large import totals.
The command now propagates card-read errors during search. Two CLI import tests
verify saved-deck preservation and exact large counts.

`mtg-headless/Cargo.toml` sets `default-run = "mtg-play"`, fixing the documented
`cargo run -p mtg-headless` command. Its main binary now rejects invalid arguments
instead of silently substituting a three-turn run. Three tests verify defaults,
valid options, errors, and help. An eight-turn terminal run succeeded; an invalid
turn count now exits with status 2 and a diagnostic.

## Storage and identity

`mtg-store/src/cards.rs` writes cards, faces, lookup keys, and FTS rows in one
transaction. Failed new-card insertion leaves no partial card; failed replacement
preserves the original. Subtype IDs use checked u16 reads, and exhausted subtype
insertion rolls back. Known subtype lookups avoid opening write transactions.

`mtg-store/src/decks.rs` uses checked u32 reads for saved card IDs/counts and reports
unknown section names instead of treating them as main-deck entries. Tests verify
negative/oversized counts, the valid u32 maximum, and unknown sections.

`mtg-store/src/identity.rs` reads identity and friend keys directly into 32-byte
arrays. Malformed blobs now produce errors instead of appearing absent or being
hidden from the friend list. `identity_or_insert` preserves the first saved
identity when callers race during startup. `mtg-app/src/net.rs` validates that a
loaded seed derives the stored public key, reports mismatches, and preserves the
stored row. Tests cover insertion/reuse, mismatches, malformed lengths, and
first-use callers with different candidate identities. A real two-connection race
starts both callers after they observe no identity, verifies both return the same
winner, and checks that reopening the SQLite file retains that winner.

## Bulk card import

`mtg-oracle/src/import.rs` has a separate transaction-scoped write path. It now
propagates SQL failures from subtype, face, card, legality, and search updates,
rolling back the whole import. Import metadata commits in the same transaction.
Unsupported rows retain their existing report/skip behavior. Subtype writes occur
once per distinct name, and subtype IDs are checked against the engine's range.
Array input must end after the array and optional whitespace.

Four regression tests in `mtg-oracle/tests/import_atomicity.rs` reproduce and fix
partial imports after face failures, ignored subtype failures, metadata failure
after data commit, and acceptance of trailing array content. The face failure
test covers both arrays and JSON Lines. All 35 existing import tests and these
four new tests pass.

## Validation

All-target/all-feature workspace Clippy with warnings denied passes after the
implementation changes. Earlier engine test lints were fixed by replacing cloned
single-item slices with borrowed slices and replacing an always-true assertion
with an actual creature-type assertion; both affected suites pass (35 tests).
README commands and contradictory transport status text were corrected.

Final `cargo test --workspace --all-features --quiet` passes 1,243 tests, with zero
failures and 41 ignored across 62 test binaries/doc-test groups. Final
`cargo clippy --workspace --all-targets --all-features --no-deps -- -D warnings`
passes. Whitespace checks pass. Loopback tests required the approved unsandboxed
test run. Ignored tests and lack of a manual desktop review remain limits; none
of this proves complete rules coverage.

Concurrent edits in engine cost/evaluation/driver code, IR trigger metadata, and
oracle compiler/compiled tests belong to other work. This audit changes only the
two engine test files mentioned above and the oracle bulk importer plus its new
test file in those areas.
