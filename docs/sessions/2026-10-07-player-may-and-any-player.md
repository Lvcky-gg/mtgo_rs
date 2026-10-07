# Other players' "may", and abilities any player may activate

Claude, card coverage round 35. 16,475 → 16,517 / 34,913 (47.3%); 28 faces from this
round, the rest Codex's. Zero faces lost against HEAD and round 34.

- `clauses::its_controller_may` generalised: any player subject `nouns::player` reads
  (except "you") followed by " may " — "each player may", "each opponent may", "target
  opponent may", "that player may", "defending player may", "that creature's controller
  may" — compiled as that player's "you may …" ("their library/hand/graveyard", "they
  control", "if they do" rewritten) inside `Effect::AsPlayer`. Refused when the clause
  says "you"/"your" itself (that is the caster, which "you" inside would misread).
  Players act one after another in turn order (CR 101.4's choice order).
- "Any player may activate this ability [but only as a sorcery]." —
  `ActivationTiming::AnyPlayer` / `AnyPlayerSorcery`; `legal_actions` offers such an
  ability on any permanent to the player with priority.
- Engine fix: `push_ability_on_stack` now sets the stack object's controller to the
  activating player (CR 602.2a); it used to keep the source's controller, which only
  differed once other players could activate.

Tests: `its_controller_may.rs` (Show and Tell puts each player's card under their own
control; Veteran Explorer has each player search and shuffle their own library),
`any_player_activates.rs` (an opponent pays and draws; an ordinary ability isn't offered).
