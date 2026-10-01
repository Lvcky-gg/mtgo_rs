# UI polish and deck artwork

Added a shared ink, slate, and gold theme to the native app, a clearer main menu,
more spacing and labels in the card gallery, styled match panels, and life badges.
Menu controls now fit narrower windows.

The deck builder offers **Choose printing** beneath gallery cards and **Art** next
to deck rows, with the same action in their context menus. An asynchronous Scryfall
search retrieves all printings for the oracle identity, including languages,
variations, and extra printings. The picker shows a scrollable grid with set,
collector-number, and language filtering. Only visible rows request images.
Gallery cards, deck names, sample-hand cards, and printing tiles offer hover
previews and an explicit **Enlarge card** action. Enlargement uses the chosen
printing and keeps selection clicks separate from inspection.

Artwork preferences currently apply to all copies of an oracle identity in that
deck. They participate in undo/redo and dirty tracking. SQLite schema v8 adds
`deck_printings`; deck replacement, including renames, saves composition and art
atomically. Cascade deletion retires old artwork preferences. Both the builder's
portable deck and the saved-deck match loader preserve printing IDs. Match card
snapshots retain artwork, including the back image for transforming/modal cards.
Oracle text and legality still come from the imported rules database.

Verification:

- App and store suites: 215 tests passed, one optional live-response test ignored.
- The optional response test was separately run successfully against a downloaded
  Scryfall response; Faithless Looting returned 135 printings in 11 languages.
- Regression coverage checks printing save/reopen/rename, undo/redo, all sections,
  saved-deck match loading, match artwork snapshots, deletion, double-faced image
  lookup, and hovering a printing without selecting it.
- App/store Clippy with `--all-targets --no-deps -- -D warnings` and diff whitespace
  checks passed. Existing unrelated dependency warnings remain in mtg-policy.
- No manual window or screenshot review was performed in this environment.
