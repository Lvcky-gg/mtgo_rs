# Dice and "create a number of" tokens

Claude, card coverage round 33. 16,420 → 16,454 / 34,913 (47.1%): 15 faces from this
round, 19 from Codex's concurrent work. Zero faces lost against HEAD and round 32.

- `Effect::RollDie { sides, outcomes: Vec<(lo, hi, Effect)>, then }` (CR 706). Rolled with
  the state's seeded RNG (restartable resolution replays the same roll);
  `Event::DieRolled { player, sides, result }`. The row holding the result resolves, then
  `then`; `Value::RollResult` in both is replaced by the number first
  (`walk::substitute_value`, via a new `Visitor::value` callback).
- Compiler: "roll a dN" in `clauses::effect` (the rest of the text becomes `then`); "the
  result" in `value_phrase`; `compile()` pre-pass groups "lo—hi | effect" rows after a
  line, compiles them (refusing rows with targets), blanks them, and `fill_die_tables`
  attaches each table to the next roll without one.
- "Create a number of <tokens> equal to <value>" / "you create a number of …".
- Not done: rows with targets, "roll two dice and choose one result", rerolls, "whenever
  you roll a die".

Tests: `dice.rs` (table row matches the logged roll across seeds; "equal to the result";
X in a row stays the spell's X), `create_number.rs` (count read after a counter; tapped
Treasures for a dead creature's power).
