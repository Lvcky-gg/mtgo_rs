# "Create X … tokens"

Claude, card coverage round 45. 16,794 → 16,841 / 34,913 (48.2%), 50 faces, all from
this round. Zero faces lost against HEAD and round 44.

Method: each blamed trigger's effect, re-probed under a known-good head ("When ~ enters,
<effect>."), failures grouped by their first words — "create x n/n" stood out.

- `clauses::create_token`: "create x <tokens>" read as "create two <tokens>" with the
  count set to `Value::X`, only while X is defined (`cx.x`: the spell's {X} cost, or a
  "where X is …" clause the enclosing effect set up). Bare "create X …" without either
  stays not understood.
- Composes with earlier rounds: Decree of Justice's cycle trigger ("you may pay {X}. If you
  do, create X …"), Endless Ranks of the Dead ("half the number of Zombies …, rounded
  down"), Darksteel Splicer ("the number of opponents you have").

Tests: `create_number.rs` (Krenko's X counted from Goblins; an {X} spell with X = 2).

## Round 52 addendum

17,003 → 17,031 (28 faces): the "create X" reading now takes any non-fixed `amount()` —
X while defined, or "that many" where the trigger supplies an amount (damage dealt,
combat damage to a player): Hornet Nest, Old Gnawbone, Tana, Darien. New
`clauses::double_counters`: "double the number of <kind> counters on <one object>" adds
that object's own count (Primordial Hydra, Growth Curve). Test: `create_number.rs`.

## Round 54 addendum

17,058 → 17,117 (62 faces): "<player> creates <tokens>" — `create_token` reads any
non-"you" subject from `nouns::player` before " creates " and sets the token's
`controller` (the engine creates the tokens for each selected player): Beast Within,
Generous Gift, Crib Swap, An Offer You Can't Refuse (the countered spell's last-known
controller). `nouns::recipient` gained "each creature and (each) planeswalker [your
opponents control / you control / you don't control]" and "each creature, planeswalker,
and battle". Tests: `create_number.rs` (+2).
