# Kinship, "it" after a self condition

Claude, card coverage round 30. 16,311 → 16,341 / 34,913 (46.8%); 18 faces from this
round, 13 from Codex's concurrent work. Zero faces lost against HEAD and round 29.

- Trigger intervening "if ~ …": with no other antecedent, "it" in the effect is the
  source ("At the beginning of your upkeep, if this land is tapped, put a storage counter
  on it.", Mana Vault's draw-step damage, Mana Bloom). Fallen Empires storage lands now
  play (with round 28's "remove any number of storage counters" mana ability).
- Kinship: `ObjectFilter::SharesCreatureTypeWith(selector)` (changelings share every
  type); the upkeep look/reveal compiles to `Let(It = top card) → If(shares a type) →
  May(reveal) → payoff`. "You may play that card" payoffs are not compiled.

Tests: `kinship.rs` (new, 4: shared type, none, changeling, declined),
`announced_mana.rs` (+2: storage land kept tapped gains a counter; tapped artifact's
draw-step damage).
