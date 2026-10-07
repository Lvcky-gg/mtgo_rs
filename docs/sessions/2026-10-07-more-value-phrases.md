# More value phrases

Claude, card coverage round 41. 16,669 → 16,711 / 34,913 (47.9%), 44 faces, all from
this round. Zero faces lost against HEAD and round 40.

Method: every "equal to <value>" phrase in the not-understood lines, by frequency, probed
in "When ~ enters, you gain life equal to <value>." (release `mtg-cards parse`).

- `Value::StartingLife` (new `PlayerState::starting_life`), `Value::LifeGainedThisTurn`
  (new `state.life_gained_amount`, kept beside `gained_life_this_turn` and cleared with
  it), `Value::ColorsAmong`.
- `value_phrase`: "twice <v>", "half <v>, rounded up/down", "<n> plus <v>", "your starting
  life total", "the amount of life you gained this turn", "the number of colors among
  <noun>", "the number of cards you've drawn this turn"; "their/his/her" possessives of
  power, toughness and mana value read like "its".
- `nouns`: "… in all graveyards" is a graveyard noun with no owner restriction.
- Left for later: "the damage dealt this way", "the life lost this way", experience
  counters, "the number of counters on it" (all kinds), exiled/discarded card values.

Tests: `value_phrases.rs` (life gained across two spells, a Lhurgoyf counting both
graveyards, twice its power, half the starting life total).

## Round 46 addendum

16,841 → 16,855 (14 faces): `value_phrase` is now a wrapper over `value_phrase_base`
adding "<n> minus <v>" and "<v> minus <n>" (`Sum` with `Negate` / a negative constant);
"the number of creatures in your party" (`per`'s singular reading). A negative difference
deals no damage and gains no life (`value_phrases.rs`, Iron Maiden).
