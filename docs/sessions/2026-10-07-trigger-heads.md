# Trigger heads: counters placed, cards leaving the graveyard, attacks you

Claude, card coverage round 42. 16,711 → 16,760 / 34,913 (48.0%), 49 faces, all from
this round. Zero faces lost against HEAD and round 41.

Method: every "When/Whenever/At …" head of a blamed sentence, by frequency, probed as
"<head>, draw a card." — failing heads are trigger conditions the compiler lacks.

- "whenever one or more <kind> counters are put on ~", "… a <kind> counter is put on a
  <noun>" → `EventPattern::CounterPlaced` (already in the engine, never compiled).
- "whenever one or more [<card type>] cards leave your graveyard," → `ZoneChange { who:
  owned by you [and type], from: Graveyard, to: None }`, once per batch.
- "whenever a <noun> attacks you [or a planeswalker you control]," → new
  `EventPattern::AttacksPlayer { who, player, or_planeswalkers }`, matched against
  `Attacked`'s defender (and `defending_player` for a planeswalker).
- detect.rs fix: `ZoneChange`'s filter falls back to the new identity when the card left
  a zone other than the battlefield, where no last-known information exists — previously
  those triggers silently never fired.

Round 43 (16,760 → 16,781, 22 faces): "deals combat damage to a player or planeswalker
/ or battle" (`AnyOf` of a player and an object `DealsDamage`), "deals [combat] damage to
a creature" — in both the self-trigger table and the noun-subject table, before the
shorter "… to a player" phrase — "you scry or surveil" (`AnyOf`), "you discard one or
more cards" (batched). Tests: `new_triggers.rs` (+2).

Round 44 (16,781 → 16,794, 13 faces): "a source deals damage to ~", "a <noun> deals
combat damage to you", "each player's first main phase", "combat on each opponent's
turn", "~ is put into your graveyard from the battlefield" (dies and owned by you), "you
cast your first spell each turn" (`NthSpellCast { n: 1 }`), "enchanted player is
attacked", "enchanted creature becomes the target of a spell or ability"; self triggers
apply `turn_qualifier` ("for the first time each turn"). Tests: `new_triggers.rs` (+1).

Round 51 (16,991 → 17,003, 13 faces): spell filters `CastFromZone`, `Kicked`,
`HasXInCost` in both `eval` and `detect`; "you cast a kicked spell / a spell from your
graveyard / from anywhere other than your hand / with {X} in its mana cost".

Next candidates from the same list: "you cast a kicked spell", "a source deals damage to ~", "enchanted player is
attacked", "enchanted creature becomes the target of …", "your next upkeep".

Tests: `new_triggers.rs`.
