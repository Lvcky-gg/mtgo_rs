# Value phrases and devotion

Claude, card coverage round 40. 16,588 → 16,669 / 34,913 (47.7%): 80 faces from this
round, 1 Codex's. Zero faces lost against HEAD and round 39.

Probing "you gain life equal to <value>" with the commonest values in not-understood
lines showed many unread values; teaching `clauses::value_phrase` them unlocked 80 faces
(devotion payoffs, domain, storage/charge artifacts, cast triggers, Inspired Sphinx).

- "your life total" → `LifeTotal(You)`.
- "your devotion to <color>[ and <color>]" → new `Value::Devotion(Vec<Color>)`
  (eval: mana symbols in costs of permanents the controller controls; colored, hybrid,
  mono-hybrid and Phyrexian symbols of those colors count once each).
- "the number of opponents you have", "… basic land types among lands you control": the
  existing `per` readings, plural.
- "the number of card types among cards in all graveyards" → `CardTypesAmong(all
  graveyards)`; `conditions::counted` reads "… in all graveyards".
- "the number of <kind> counters on ~ / it" → `Counters`.
- "that spell's …" → `cx.it`, refused when that is still the source: a "where X is that
  spell's mana value" is read before "counter target spell" (Draining Whelk stays out).

Tests: `value_phrases.rs`.
