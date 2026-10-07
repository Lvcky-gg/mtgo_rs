# "Its controller may …" (AsPlayer)

Claude, card coverage round 34. 16,454 → 16,475 / 34,913 (47.2%): 14 faces from this
round, 9 from Codex's concurrent work. Zero faces lost against HEAD and round 33.

- `Effect::AsPlayer { who: Selector, body }`: resolves `body` once per player `who`
  picks, with `rc.controller` set to that player — so "you", `May` prompts, `ChosenBy`
  searches and `Shuffle` are theirs. Targets and bindings stay the enclosing effect's.
- `clauses::its_controller_may`: "its controller may <clause>" → the "you may" grammar on
  the clause with "their library/hand" read as "your …", wrapped in `AsPlayer` with
  `who = ControllerOf(it)` (last-known controller after a destroy or exile).
- Cards: Path to Exile, Assassin's Trophy, Ghost Quarter, Erode, Geomancer's Gambit,
  Cleansing Wildfire, Price of Freedom, White Orchid Phantom, Sandworm, Synapse and
  Brood Sliver, 70,000 Light-Years from Home, Sundering Eruption, Avengers Disassembled.
- Not done: "for each land destroyed this way, its controller may …", "each player may …".

Tests: `its_controller_may.rs` — Path to Exile's target's controller finds a basic from
their own library (tapped, their library shuffled); declining finds nothing.
