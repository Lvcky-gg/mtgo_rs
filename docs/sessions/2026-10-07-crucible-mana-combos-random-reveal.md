# Crucible, mana in any combination, revealing at random

Claude, card coverage round 39. 16,571 → 16,588 / 34,913 (47.5%): 13 faces from this
round, 4 Codex's. Zero faces lost against HEAD and round 38.

- `Restriction::PlayLandsFromGraveyard`: in `legal_actions`, the player's graveyard lands
  join the candidates when some effect they control grants it, and pass the graveyard
  check (which otherwise wants a graveyard-cast ability); still one land per turn.
- `Effect::AddManaAnyCombination { amount }` (`add_mana_effect`: "add two/X mana in any
  combination of colors"): one color asked per mana as it resolves. `activated()` refuses
  an effect containing it, since an ability that adds mana is a mana ability and can't
  ask. Mana abilities with this text remain unsupported.
- `Effect::RevealRandom { who }` (`clauses::reveal_random`): the seeded RNG picks a hand
  card, `Revealed` is logged, and it's bound as `Binding::It`; "that player" becomes the
  revealer. "The revealed card's …" always reads `Bound(It)` — a "where X is the revealed
  card's mana value" is parsed before the reveal clause, when "it" was still the source.

Tests: `graveyard_lands.rs` (a land played from the graveyard as the turn's land; none
without the permission), `announced_mana.rs` (Manamorphose's white and green),
`reveal_random.rs` (damage and +X/+X equal to the revealed card's mana value — the second
caught X reading the enchantment's own mana value).
