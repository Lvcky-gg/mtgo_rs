# Reject command sections the app cannot preserve

The app importer now rejects companion designation and multiple commanders before
writing any deck. Previously companions became commanders and only the first
commander survived. These roles cannot be faithfully represented in the current
app DeckSpec, which stores one commander and no companion designation.

Counted commander entries are checked too. Saved-deck conversion rejects multiple
commander entries or a commander count other than one. Import errors retain the
pasted text and existing saved decks for correction.

A regression covers two commander lines, a two-copy commander line, and a
companion line, verifying that an existing same-name deck is untouched. The saved
count regression also checks a two-copy commander is rejected before play.

Validation: all five deck tests pass. Changes remain uncommitted.
