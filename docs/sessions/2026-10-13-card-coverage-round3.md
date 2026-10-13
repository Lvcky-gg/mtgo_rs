# Card coverage, round 3 — 2026-10-13

Continued the existing uncommitted coverage work. The same 34,914-card database
snapshot now reports 17,567 playable cards (50.3%), up from 17,562: five additions,
zero losses. Comparing the complete name checks against round 2 identifies exactly
Tainted Pact, Final Fortune, Last Chance, Warrior's Oath, and Tataru Taru. The check
command retains its pre-existing five-card difference from the coverage command
(17,572 versus 17,567). Together, all three rounds add 143 cards over the fresh HEAD
baseline of 17,424.

Tainted Pact has a dedicated resolution loop. Each card is exiled before offering
it for the hand; accepting ends the process. A repeated name stops the process
without offering the duplicate. Only cards exiled by this resolution count. Split
cards contribute both names; double-faced cards contribute only the front name.
An empty library ends the loop without a draw attempt.

Final Fortune and its two equivalent spells create a uniquely marked extra turn
and a delayed loss trigger for that turn's end step. Later-created extra turns
retain their normal ordering and do not inherit the loss. Skipping the marked turn
does not move its loss onto an ordinary turn. Countering the original spell creates
neither an extra turn nor a delayed trigger; countering the delayed triggered
ability prevents the loss. Optional serialized metadata preserves existing
unmarked extra-turn diagnostic snapshots.

Tataru Taru's ability word and intervening condition now compile. The condition
compares the active player with the player bound by the draw event. Its entering
ability lets the targeted opponent decide whether to draw; qualifying opponent
draws create a tapped Treasure only once each turn.

The draw-batch regression exposed a pre-existing once-per-turn trigger bug:
detection could queue several occurrences before placement recorded the first.
Detection now checks intervening conditions before deduplicating limited triggers,
and placement rechecks the limit after earlier queued triggers have consumed it.
Declining an optional effect still consumes a trigger limit, while the existing
"Do this only once each turn" behavior still counts only doing the effect.

Added 14 compiled-card regressions: five each for Tainted Pact and Final Fortune,
three for Tataru, and one independent optional-effect/draw-batch trigger-limit test.
Existing regression fixtures need no changes in this round.

Validation: `cargo test --workspace --quiet` (including the complete replay and
regression corpus), `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --all --check`, and `git diff --check` pass. The workspace tests ran
outside the sandbox for their existing loopback networking tests. All work remains
uncommitted.
