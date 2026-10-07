# Exhaust, boast, and exiling the source as a cost

Claude, card coverage round 48. 16,895 → 16,936 / 34,913 (48.5%), 41 faces, all from
this round. Zero faces lost against HEAD and round 47.

Method: the "<cost>:" of every not-understood activated line, probed as "<cost>: Draw a
card." and ranked by frequency.

- "Exile this artifact / this creature / ~" as a cost →
  `AdditionalCost::ExileFrom { zone: Battlefield, filter: IsSelf }`; payable while on the
  battlefield (`cost::additional_payable`), paid by the generalized self-exile step in
  `pay_ability_cost`.
- Exhaust — `ActivationTiming::Exhaust`; `state.activated_ever` (never cleared) records
  each `(object, ability)` activated, and `legal_actions` skips an exhausted one. A new
  object (after a zone change) can exhaust again.
- Boast — "Boast — <cost>: <effect>" is `InstantOncePerTurn` with a cost condition that
  the source is among the creatures that attacked this turn.
- Not done: Power-up (exact rules not verified), snow mana {S}, waterbend, "discard a card
  at random" as a cost, "{TK}" tickets.

Tests: `activation_limits.rs`.

## Round 49 addendum

16,936 → 16,970 (34 faces): `cost()` now reads "return <n> <noun> you control to its
owner's hand" (`return_hand_cost`, Codex's `ReturnToHand`, chosen and paid through the
announcement as for alternative costs), "return ~ to its owner's hand" (`ReturnToHand`
filtered to `IsSelf`), "mill N cards" (`AdditionalCost::Mill`; CR 701.13b — not payable
with fewer cards in the library) and "put N <kind> counters on ~"
(`AdditionalCost::PutCounters`). Tests: `activation_limits.rs` (+2).
