# Tutor resolution and random discard

Gamble's printed text did not compile: the search parser required putting the
card into hand to be followed immediately by shuffling. Gamble discards a card
at random between those steps, so the entire spell effect was unparsed. An
end-to-end printed-text regression reproduced that failure before the fix.

The compiler now preserves intervening comma-separated actions and handles
shuffling as its own clause. Gamble resolves in printed order: search, move the
chosen card into hand, randomly discard from the resulting hand, then shuffle.
The random discard uses the existing seeded engine generator and asks no extra
selection question. The tutored card can itself be discarded.

Other tutor fixes:

- Unrestricted searches require the requested number of cards when available.
  Filtered searches and explicit optional counts still allow finding none.
- Worldly Tutor's printed `put the card on top` wording is accepted alongside
  `put that card on top` and `put it on top`.
- Exact multi-card searches and explicit `shuffle your library` wording compile.

Gamble, Demonic Tutor, Worldly Tutor, and Mystical Tutor wording was checked
against the installed oracle database using a read-only immutable connection.
Regression fixture card names remain invented.

Six new end-to-end tests cover move/discard/shuffle event order with an otherwise
empty hand, seeded replay and both possible random-discard outcomes, empty-library
resolution, required versus optional selection, Worldly Tutor's top placement,
and moving two selected cards into hand. Picker views disclose candidates to the
searching player without exposing their library to the opponent.

Checks:

```sh
cargo test -p mtg-oracle --test compiled search:: --offline
cargo test -p mtg-oracle -p mtg-engine -p mtg-policy --offline --quiet
cargo clippy -p mtg-oracle --all-targets --offline -- -D warnings
cargo build -p mtg-app --bin mtg-gui --offline
```

All 10 search tests pass. The broader suite passed 1,003 tests with 38 ignored;
Clippy passed. The GUI was rebuilt. Start a new match with the updated binary;
existing imported cards compile from stored text when loaded, so reimporting
decks is unnecessary.
