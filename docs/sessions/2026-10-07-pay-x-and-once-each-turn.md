# "You may pay {X}" and "Do this only once each turn"

Claude, card coverage round 36. 16,517 → 16,546 / 34,913 (47.4%): 14 faces from this
round, the rest Codex's concurrent work. Zero faces lost against HEAD and round 35.

- `Effect::MayPay` with {X}: after "yes", asks `ChooseX` up to the most payable, pays it,
  and substitutes the number for `Value::X` in the body before resolving it — so a
  "when you do" reflexive trigger, resolving later on its own, keeps the X. Compiler:
  `clauses::may` turns `cx.x` on while parsing that body; `reflexive` copies `cx.x`.
- "Do this only once each turn." (`compile_line`, on a "you may" trigger): the effect is
  wrapped in `Effect::OnceEachTurn` and `Effect::MarkOnceEachTurn` goes at the start of
  the yes branch (`mark_once_each_turn`); `state.done_once_this_turn` (by source object)
  is cleared with the other per-turn bookkeeping. Declining doesn't count (CR 603 —
  the ability still triggers each time).

Tests: `pay_x.rs` (pay X=2 draws two; "when you do" puts X counters; declining pays
nothing), `once_each_turn.rs` (decline, accept, already done, and fresh next turn).
