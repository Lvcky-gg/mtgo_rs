# Stack threat identification and counter matching

Codex follow-up to the user's request for threat identification and appropriate
responses. Existing Claude and Codex working changes retained.

## Core stack-order bug

`PlayerView::stack` is populated by `state.objects_in(Stack)`, which is top first.
Its comment incorrectly said bottom first. Both `Bot::counter` and `Bot::targets`
read `.last()`, so a multi-object stack made them inspect the oldest object.
They now use `.first()`; the view comment is corrected without changing serialized
data or engine behavior.

This prevents an opposing spell underneath the bot's own counter from prompting
another counter. It also reads the announcing counter's effect metadata rather
than the oldest spell's targeting preference. Counter targeting preserves the
object immediately below the announcing counter as the intended threat, avoiding
switching from a lethal instant to a large creature spell beneath it. Search
target alternatives remain free to explore other legal choices.

## Response matching

`bot/stack_strategy.rs` caches single-target hard and tax counters and visible
unconditional counter-protection statics from compiled IR. Counter selection:

- Checks the actual opposing stack object's target filter, including spell versus
  ability and creature versus noncreature. A legal target elsewhere on the stack
  does not imply that the chosen response can answer the top object.
- Recognizes self uncounterability and battlefield statics such as "Spells you
  control can't be countered", interpreting "you" as the protecting permanent's
  controller. Opponent-owned protection does not protect the wrong player's spells.
- Holds generic-tax counters when public floating mana or simple untapped tap-mana
  sources can cover the tax. Counts alternative tapping abilities on one permanent
  only once. Conditional or unknown outputs and creatures are not counted as certain
  sources. Among remaining responses, hard counters rank ahead of uncertain ones,
  then the existing mana-value/resource proxy breaks ties.
- Supports ability counters against opposing activated/triggered abilities rather
  than rejecting every ability on the stack. Ordinary spell counters are excluded
  from that job.

`known_filter_for` shares the conservative projected-state matcher with tactics,
adding explicit source-controller context and spell/ability predicates.

## Verification

Nine real compiled-card regressions in `bot_stack_threats.rs` cover payable tax
using both floated and untapped mana; holding a payable soft counter; using a tax
counter with insufficient known mana; one source with two tapping alternatives;
self and controller-specific counter protection plus the wrong-controller negative
case; stopping a lethal activated ability with an ability counter; preserving an
ordinary counter; narrow counter versus an actual creature threat on a two-spell
stack; preserving a second counter above the bot's own response; and countering the
lethal top instant instead of an 8/8 underneath it.

Latest full compiled-card suite: 1,444 pass / 15 ignored. Policy54, headless bot1,
package formatting, diff checks, and policy all-target no-deps Clippy pass.

## Limits

This does not yet decide which harmless opposing spells deserve to resolve in
order to save counters for future threats. Unknown spell-counter patterns retain
the previous fallback; ability-counter targeting requires known matching filters.
Counter protection is derived from unconditional printed statics, not every
continuous or temporary grant. Mana estimates omit hidden/conditional sources and
do not account for every restriction on using visible mana sources. An insufficient
estimate is uncertain, not proof that a tax cannot be paid; paid generic counters
are deliberately conservative. Mana value remains a proxy for the bot's own actual
response cost. No strength benchmark was run.

Files: `bot/stack_strategy.rs`, `bot/{mod,tactics,cast_strategy}.rs`, the view
comment in `mtg-engine/src/view.rs`, and compiled `bot_stack_threats.rs` registration.
