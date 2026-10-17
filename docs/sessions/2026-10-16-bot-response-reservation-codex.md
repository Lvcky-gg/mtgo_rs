# Reserving interaction and answering engines promptly

Codex follow-up to threat identification and appropriate responses. Existing
Claude search/tuning and card coverage changes retained.

## Counter preservation

The heuristic no longer automatically counters a narrowly recognized low-impact
life-gain effect. It preserves interaction when the entire effect is fixed
self life gain totaling at most three, the opponent already has at least 25 life,
and there is no visible reason those points matter. Empty effects also pass.
Unknown, mixed and larger effects retain previous response behavior.

The exception is deliberately conservative. A visible life-gain trigger (including
combined events), life-payment engine, life-total-dependent ability, life-gained-
this-turn condition, potential alternate win, or unidentified battlefield card
prevents dismissal. Potential friendly combat damage that would be lethal before
the gain but not afterward also keeps the effect answerable. All information comes
from the projected view and known printed metadata; no hidden hand inspection.
Sequence amounts are summed, not judged one clause at a time.

An actual two-spell scenario now lets high-life small-gain bait resolve and uses
the retained last counter on a subsequent lethal life-loss spell. The lookahead
search also chooses Pass on that bait in the seeded regression. Negative controls
cover low life, a large gain, several small gains totaling more than three, a
gain-plus-draw spell, lethal-range preservation, triggered drain, paying life for
cards, a life-gain-enabled end-step draw, and a life-based alternate win.

## Proactive permanent responses

`engine_response` shares target matching with combat responses and prioritizes
an effective answer to an opposing repeatable draw/win engine (existing engine
score at least 32) on an empty stack. It can act on the opponent's turn, instead
of waiting for its own main phase, and before spending mana on a new vanilla
threat. A source's removal is not treated as a way to stop an ability already
on the stack.

Combat and engine responses now include engine-offered activated removal abilities,
using each ability's own target specification and response profile. A simple cost
proxy counts printed mana, spending a spell card, and additional ability costs;
this prefers a free reusable tap-removal ability over an otherwise equal spell.
Effectiveness still comes first, so destruction does not substitute for exile
against indestructible. Existing Fog and recipient shields suppress combat urgency.

Three real-game regressions show removal of an opposing draw engine before the
bot's turn; reusable removal stopping lethal while keeping the spell in hand;
and effective exile of an indestructible draw engine before casting an available
seven-mana creature.

## Validation and limits

Nine new compiled-card tests, with multiple context cases. Current full card suite:
1,469 pass / 15 ignored; policy54 and headless bot1 pass. Formatting, diff checks
and policy all-target Clippy with `--no-deps -D warnings` pass.

The 25-life/three-life and engine-score cutoffs are conservative hand heuristics,
not fitted thresholds. This does not classify every innocuous spell or discover
arbitrary combos. Unknown effects stay answerable. Source matching and damage
effectiveness retain the printed-stat/keyword limitations described in the earlier
threat handoff. Activated costs use a resource proxy and do not solve the value of
every sacrifice, blocker or alternative payment. No playing-strength benchmark run.

Implementation: `bot/stack_strategy.rs`, `bot/tactics.rs`, `bot/cast_strategy.rs`.
Regressions: compiled `bot_stack_threats.rs` and `bot_threats.rs`.
