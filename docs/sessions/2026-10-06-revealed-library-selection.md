# Revealed library selection card support

Codex extended `clauses::dig` for reveal-the-top instructions, taking all
matching cards, `revealed this way`, joined destinations, and `the rest of the
revealed cards`. Existing `Reveal` and `Dig` operations preserve public reveals,
optional versus mandatory selections, filtering and destinations. Claude's
concurrent noun-parser support handles `and/or` alternatives such as Pieces of
the Puzzle; Codex left that parser and casting/cost code untouched.

## Battlefield entry correction

`Effect::Dig` previously moved selected cards straight onto the battlefield
without calling normal entry processing. It now asks entry choices and applies
entry replacements just as `MoveZone` does. This fixes counters, tapped entry and
chosen colors for existing Collected Company-style selection as well as newly
accepted Muxus-style selection. The change is confined to the Dig match arm in
`mtg-engine/src/resolve.rs`.

## Measured coverage

A frozen copy of all current crates in `/tmp/mtgo-reveal-comparison` was built
and measured against the same isolated 34,913-card SQLite snapshot. The only
source difference between the two coverage builds was this batch's `dig`
compiler extension. Claude's concurrent changes were identical in both builds:
**15,468 → 15,494 playable cards (44.4%), +26, zero lost card faces**.

Artifacts: `/tmp/codex-reveal-frozen-before.txt` and
`/tmp/codex-reveal-frozen-after.txt`. The user's original database was not changed;
no card reimport is required. This is complete printed-text compilation coverage,
not an exhaustive gameplay audit of every accepted card or interaction.

## Validation

Eight new regression tests, fourteen total in `compiled/dig.rs`, cover:

- Mulch's mandatory land selection, all-card public reveals and reveal-before-move order.
- Grisly Salvage's optional creature/land choice, top-five boundary, opponent-library
  exclusion, and malformed duplicate/unrelated selections.
- Goblin Ringleader's subtype filter including noncreature kindred Goblin cards.
- Pieces of the Puzzle's shared two-card limit across instants and sorceries.
- Enshrined Memories at X=0, positive X and X exceeding the library size.
- Muxus's creature/subtype/mana-value filters, entry counters, tapped entry and ETB draw.
- Rejection of separate type quotas, total mana-value constraints, explicit tapped
  destinations, undefined X and unrevealed `revealed this way` references.
- Normal color-choice processing for selected permanents and no duplicated reveals
  when entry choices suspend and replay resolution.

Passed:

- `cargo test -p mtg-oracle --test compiled dig:: --offline --quiet`
- `cargo test -p mtg-oracle -p mtg-engine -p mtg-ir -p mtg-policy --offline --quiet`
  (1,073 passed, 38 existing tests ignored).
- `cargo clippy -p mtg-oracle -p mtg-engine --all-targets --offline -- -D warnings`
- `cargo build -p mtg-app --bin mtg-gui --offline --quiet`
- `git diff --check`

## Newly supported cards

- Adéwalé, Breaker of Chains
- Beast Hunt
- Borborygmos Enraged
- Brass Herald
- Chromescale Drake
- Commune with the Gods
- Elder Pine of Jukai
- Enlistment Officer
- Enshrined Memories
- Goblin Ringleader
- Grave Defiler
- Grisly Salvage
- Kavu Howler
- Kruphix's Insight
- Malevolent Rumble
- Merfolk Wayfinder
- Mulch
- Muxus, Goblin Grandee
- Pieces of the Puzzle
- Satyr Wayfinder
- Scout the Borders
- Sylvan Messenger
- Tidal Courier
- Torsten, Founder of Benalia
- Tracker's Instincts
- Vessel of Nascency
