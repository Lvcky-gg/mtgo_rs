# Threat assessment and appropriate responses

Codex contribution for the user's request to identify threats and choose appropriate
responses, alongside Claude's existing bot/search and card work.

## Behavior

`bot/tactics.rs` caches response capabilities and recurring ability value from
compiled IR, without a card-name list. Threat scores consider combat damage,
evasion, deathtouch, loyalty, and repeatable draw/token/mana/win effects. Declared
attacks get urgency, and player survival takes priority over planeswalker attacks.
Self enters/leaves/dies card advantage is excluded from recurring engine value.

Hostile target ranking now uses response effectiveness and threat score before
mana value. Fixed burn prefers a kill over wounding an oversized creature, and
lethal player damage outranks creature removal. Destruction avoids printed
indestructible; exile and bounce remain usable against it. Simple exile/bounce
from the battlefield are hostile; graveyard-to-hand returns are not misclassified.

The heuristic can cast effective instant removal during declared combat, including
on an opponent's turn, instead of always passing. It considers the engine's offered
actions, conservatively matches known target filters, chooses a response to the
most urgent answerable attacker, and favors lower mana value among equal responses.
It holds creature-only burn that cannot kill and destruction that cannot answer
an opposing permanent. Counterspell selection favors lower mana value among
otherwise usable counters. Global combat prevention suppresses emergency removal.

Search material evaluation shares the recurring-ability scores through a tunable
`Weights::engine` (default 0.15). It filters printed ability IDs against the current
layer-computed abilities so an ability that was removed is not counted.

## Verification

Ten compiled-card regressions in `bot_threats.rs` cover:

- Cheap repeatable draw engine versus expensive vanilla creature, both heuristic
  and search target selection, and a spent ETB draw effect as a negative control.
- Burn that kills versus burn that only wounds; lethal damage to the player.
- Destruction versus exile against indestructible, and holding ineffective removal.
- Removal before lethal combat damage and prioritizing the player over a larger
  attacker aimed at a planeswalker.
- Narrow color-restricted removal held during an unrelated attack.
- Cheaper effective counterspell chosen despite the expensive spell's lower object ID.

Latest checks: compiled-card suite 1,430 pass / 15 ignored; policy 54 pass;
headless bot gameplay 1 pass; package formatting, diff check, and policy all-target
Clippy with `--no-deps -D warnings` pass. Dependency-inclusive Clippy initially
reported two existing `collapsible_if` lints in engine `resolve.rs`; not changed here.
Two concurrently added `different_names` fixtures initially failed in the broad
suite; their author corrected them during this run, and the fresh broad run passes.

## Limits and next work

These are hand-weighted priorities, not a calibrated threat predictor. Arbitrary
combos, static lock pieces and replacement-based win conditions are not recognized
comprehensively. Combat danger uses total visible damage without solving blocks.
The heuristic still uses printed stats plus counters/damage and printed defensive
keywords, so continuous effects may change actual response effectiveness. Emergency
selection handles direct single-target fixed damage, destroy, exile and bounce;
conditional/modal/variable responses remain for existing heuristic/search handling.
Protection is conservatively skipped; unfamiliar target predicates are not guessed.
Actual target choices still come from the engine. Lower mana value is only a proxy
for counterspell cost and does not model whether an opponent can pay a soft counter.
No playing-strength benchmark was run.

Files: `bot/tactics.rs`, `bot/mod.rs`, `bot/cast_strategy.rs`, `search/eval.rs`,
and compiled `bot_threats.rs` registration in `compiled/main.rs`. Existing work retained.
