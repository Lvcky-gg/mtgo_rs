# "Becomes the creature type / color of your choice"

Claude, card coverage round 37. 16,546 → 16,562 / 34,913 (47.4%): 15 faces from this
round, 2 Codex's. Zero faces lost against HEAD and round 36.

- `Modification::SetCreatureTypes(Vec<Subtype>)` — layer 4; removes the creature types
  (by `is_creature_type` on the subtype's name) and changeling, keeps other subtypes.
- `Modification::BecomesChosen(EntryChoice)` — a placeholder that `Effect::Continuous`
  replaces as it begins with `SetCreatureTypes([chosen])` / `SetColors([chosen])` from the
  source's `chosen_subtype` / `chosen_color` (set by an enclosing `Effect::Choose`).
- `clauses::becomes_chosen`: "<object> becomes the creature type of your choice / the color
  of your choice until end of turn" (wrapped in `Effect::Choose`), and "… becomes the
  chosen type/color until end of turn" (what "choose a creature type. … becomes that type"
  reads as).
- Not done: "choose a creature type other than Wall" (needs an excluded option), "target
  spell or permanent" nouns, basic land types.

Tests: `choose_on_resolution.rs` — a Bear that becomes an Elf is no longer a Bear; a red
creature that becomes blue is only blue.
