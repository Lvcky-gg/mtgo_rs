# Targeted players placing hand cards in their libraries

The compiler accepts “target opponent puts a card from their hand on top of their
library”, and fixed plural top/bottom variants with optional “in any order”. The
player target feeds the existing ChosenBy selector, so the chosen player sees and
selects their own hand. The ownership filter also scopes candidate cards. Existing
MoveZone behavior asks the owner for library order and moves to that owner's library.
Group subjects remain excluded because ChosenBy currently selects for one player.

Two regressions cover top and bottom ordering, targeted-player and targeted-opponent
wording, exclusion of the caster's hand even in malformed selections, and empty-hand
behavior. Validation: 1223 engine/Oracle tests pass, 38 ignored;
workspace all-target/all-feature Clippy and formatting pass. Live audit:
16,432 / 34,913 cards playable (47.1%), up four from the previous snapshot.
