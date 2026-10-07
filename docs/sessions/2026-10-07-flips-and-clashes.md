# Coin flips and clash

Claude, card coverage round 31. 16,341 → 16,388 / 34,913 (46.9%): 37 faces from this
round, 10 from Codex's concurrent work. Zero faces lost against HEAD and round 30.

- `Effect::FlipCoin { win, lose }` (CR 705): "Flip a coin. If you win the flip, … If you
  lose the flip, … / Otherwise, …", and the lose-only form. Uses `state.rng`, so a
  resolution restarted from its snapshot to ask a question flips the same way.
  `Event::CoinFlipped { player, won }`.
- `Effect::Clash { win, lose }` (CR 701.23): "Clash with an opponent. If you win, …
  Otherwise, …". The opponent is implied with one (asked with several); both top cards
  are revealed; each player chooses top or bottom (yes/no); a player wins with a mana
  value higher than every other revealed card. `Event::Clashed { player, won }` for
  each clashing player (for "whenever you clash" later).
- Compiler: `clauses::gamble`, tried first in each sentence of `clauses::effect`.
- Not done: "whenever you clash" triggers, "you may clash", flips "for each", "flip until".

Tests: `flips_and_clashes.rs` (new, 3): winning a clash returns the spell to hand (not the
graveyard) and the opponent's bottom choice is honoured; losing keeps it in the
graveyard; a flip's branch matches the logged result across seeds, both outcomes seen.
