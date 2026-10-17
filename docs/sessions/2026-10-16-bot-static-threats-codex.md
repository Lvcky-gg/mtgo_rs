# Static threats and replacement-win responses

Codex follow-up to identifying threats and appropriate responses. Changes stay in
heuristic `bot/` and compiled-card tests, respecting Claude's ownership of search
allocation, evaluation, tuning and engine speed.

`bot/strategic.rs` caches supported unconditional static restrictions and search/draw
capabilities from compiled IR. No card-name threat list is used. Context priorities:

- Casting bans are urgent when they affect this viewer and match a visible hand or
  command-zone card. Ownership/controller context matters. Land-count casting caps
  compare printed mana value with lands, not mana from rocks.
- Escape enablers are urgent when their controller has matching graveyard cards and
  sufficient separate exile fuel. The card being cast is not counted as its own
  fuel. Another player's graveyard and an all-land graveyard do not enable Breach.
- Search prevention and search theft become urgent when our visible hand/command
  cards provide a search plan, or an activated battlefield ability provides one.
  A battlefield creature's spent one-shot enters tutor is not an ongoing search plan.
- Empty-library replacement wins become more urgent as the controller's library
  shrinks. If a supported pending draw can make that player win from an empty
  library, instant removal of the enabler is considered while the draw is on the
  stack. This differs from a pending activated "you win" ability: removing that
  ability's source cannot counter it, and the bot does not waste removal on it.

The scores feed hostile target ranking and proactive permanent responses, including
engine-offered activated removal when a casting lock prohibits removal spells.
The projected predicate matcher gained zone membership, ownership where exposed by
zone/controller, and an explicit source context for "other" escape-cost cards.
Unknown predicates remain unknown even under negation.

Eight compiled-card tests in `bot_static_threats.rs`, with context matrices, verify
commander unlocking; land-count caps versus rocks; active/inert/wrong-owner/all-land
escape resources; search denial/theft with no tutor, a tutor in hand, an activated
tutor, and a spent ETB tutor; replacement-win readiness; an actual winning defense
against Laboratory Maniac plus empty-library draw; the activated-win negative case;
and an activated removal answer that unlocks otherwise uncastable removal.

## Boundaries

Scores and library cutoffs are hand heuristics, not fitted values. Graveyard scoring
checks card/fuel resources, not complete mana affordability. Search-plan discovery
is conservative and does not model every recurring triggered tutor. Static effects
with conditions and temporary/layer-granted changes are not comprehensively handled.
Pending-draw discovery recognizes supported IR shapes and can conservatively see a
potential draw in a conditional/modal effect; it cannot read unexposed chosen modes.
Capped per-turn casting rules above zero are left to search because the projected
view lacks the turn's casting history. Arbitrary combos and lock pieces remain outside
this classifier. Search evaluation coefficients/features were not changed.

The Demo arena lacks these static-threat patterns. Its searcher-versus-heuristic
score is a compatibility measurement, not evidence that this change increases
playing strength. A representative threat-heavy arena deck is still needed for
that claim.

## Verification and arena

Final compiled-card suite: 1,486 pass / 15 ignored, including all eight new static
threat tests and their positive/negative context cases. Policy54 and headless bot1
pass. Formatting, diff check and policy all-target no-deps Clippy pass.

Paired measurement before and after the change:

```text
cargo run --release -p mtg-app --example arena -- \
  --seeds 8 --playouts 64 --horizon 1 --deck demo --threads 2
```

Both produced **68.8% ± 17.9%, +137 Elo [+6, +325]** for searcher versus heuristic:
16 games, zero adjudications, 15.3 mean turns, 3,096 searched decisions and 39
overrides. Before: 126.1 seconds total / 75.0 ms per searched decision. Final:
128.6 seconds / 76.5 ms. These single timing observations are not a performance
claim, and both sides use the current heuristic; this is not a head-to-head
comparison of the previous heuristic against the changed one. The baseline
heuristic-versus-itself runs score 50% as expected.

No overall playing-strength gain is established by this arena result. The new
decisions are verified by actual game outcomes and context tests; measuring their
competitive value requires decks containing these locks and enablers.
