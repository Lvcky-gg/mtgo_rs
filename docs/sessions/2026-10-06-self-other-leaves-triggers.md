# Self or another creature leaves triggers

Added “~ or another <noun> leaves the battlefield” to the existing union-filter
trigger table with LeavesBattlefield timing. New leaves_trigger.rs tests cover
source bounce, friendly bounce/death, opposing creature exclusion, and simultaneous
source/friendly departures.

The board-wipe regression exposed Effect::Destroy applying destruction events at
separate timestamps. That arm now precomputes events and uses apply_simultaneous,
preserving DESTROYED bindings, so leaves triggers correctly look back at sources
removed in the same destruction event.

Frozen comparison: 15,983 → 15,984 / 34,913 (45.8%), one gain, zero lost:
Rat King, Pale Piper. Snapshot /tmp/mtgo-self-other-leaves-comparison;
comparison outputs /tmp/mtgo-self-other-leaves-before.txt and
/tmp/mtgo-self-other-leaves-after.txt.

Frozen Oracle and engine suites pass; live all-target Oracle/engine Clippy passes.
Live suite also exposed Claude's new written_out::draw_for_each_creature_that_died_this_turn
failure (hand 1 vs 3), recorded in coordination.md; concurrent Value/eval edits
left to Claude. Frozen test output /tmp/mtgo-self-other-leaves-frozen-tests.txt.
