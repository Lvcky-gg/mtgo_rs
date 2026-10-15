# Deck card coverage — 2026-10-15

Saved the user's three commander decks as a deduplicated, ordered 209-card
priority list in `docs/card-coverage/deck-priorities.txt`. The two concatenated
commander/deck boundaries were separated; GUT SHOT was normalized to Gut Shot.
The check command accepts this list directly.

Using a SQLite backup of the current local catalog, deck compilation coverage
increases from 158 to 162 / 209. Newly playable: Soul-Guide Lantern, Displacer
Kitten, Sigil of Sleep, and Sokenzan, Crucible of Defiance. The complete remaining
blocker report is in `docs/card-coverage/deck-priorities-report.txt`.

Whole-graveyard nouns now select every opponent's cards by ownership without
creating target slots. Also supports all opponents' graveyards and each player's
graveyard. The `they` pronoun refers to the whole token group just created, and
Avoidance is recognized as an ability label. Target filters can refer to
creatures "that player controls". Target announcement and resolution checks
retain the stack ability's event bindings, allowing Sigil of Sleep to identify
the damaged player.

Five compiled gameplay regressions cover multiplayer graveyard exile preserving
your own cards, blinking a stolen permanent back to its owner, declining Kitten's
optional target, haste on both Sokenzan tokens expiring at end of turn, and Sigil
returning a creature controlled by the damaged player to its owner's hand,
including a creature owned by the other player.

Six names are absent from this catalog: Balin's Tomb, Widow-Making Infiltrator,
Dol Amroth, Hope's Aero Magic, Mayor Tong of Chin Village, and Slimed. They remain
explicit UNKNOWN entries rather than being silently skipped or approximated.
There are 41 other blocked deck cards. Commander coverage: Thrasios compiles;
Vial Smasher, Araumi, and Cayth remain blocked.

Current whole-catalog compilation coverage: 17,925 / 34,954 (51.3%). This catalog
has more entries than the previous session's snapshot, so this is not a directly
comparable global delta. Compilation coverage is not a comprehensive rules audit.

Changes preserve the existing uncommitted coverage work and remain uncommitted.

Validation: all five new compiled regressions pass; `cargo test --workspace
--quiet` passes outside the sandbox for the existing loopback networking tests;
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --
--check`, and `git diff --check` pass. No existing fixture was updated.
