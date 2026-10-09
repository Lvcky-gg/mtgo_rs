# Leaving a multiplayer game (CR 800.4a/800.4d)

Continues the verification roadmap's "CR 800.4 object/control-effect cleanup" gap.
Before this, a player who lost a multiplayer game kept every object they owned:
their permanents stayed on the battlefield, their abilities stayed on the stack and
their triggers still went on the stack.

Golden tests (`crates/mtg-verify/tests/leave_game.rs`) were written from the
2026-09-25 Comprehensive Rules text first; all six propositions failed on the
unchanged engine and the control case passed. They are Builder-authored, not an
independent Rules Lawyer's.

Engine (`Engine::leave_game`, called after a loss by state-based action and after
a concession, skipped when fewer than two players remain):

1. Layer-2 `Control(You)` effects whose recipient is the departed player end, and
   every object they own, in any zone, ceases to exist — one simultaneous batch.
2. Their abilities and spell copies on the stack cease to exist; spells and
   permanents still under their control are exiled (new identities, owners keep
   the cards).
3. Their triggers are dropped when pending triggers are batched for placement
   (800.4d), plus anything already queued.

`CeasedToExist` on a battlefield object now does the same leave-the-battlefield
cleanup as a zone change (attachments, combat, source-bound effects, last-known
information); `apply::left_battlefield` is shared by both.

The first 800.4d fixture passed for the wrong reason: the departed player's own
card left the game before detection, so its trigger vanished incidentally. A
mutant removing the filter survived. The fixture now uses a creature another
player owns, which the mutant fails.

Evidence: 7 golden tests; production mutation smoke 8/8 killed (four new 800.4
mutants); 22-check PR campaign passes; workspace all-feature tests 1,842 passed,
44 ignored across 74 suites; all-target Clippy clean. The `multiplayer_elimination`
replay's checkpoints were refreshed after the golden tests passed — the only state
difference is the departed player's library leaving the game. The seed-484
regression fixture is unchanged (empty batches don't advance timestamps).

Open within 800.4: control changes / tokens / permanents arriving for a departed
player (800.4b), 800.4c, combat damage to a departed player (800.4e), costs and
choices owed by departed players (800.4f–h), and duration handling (800.4m).
No primitive is promoted; closed-alpha assessments remain unknown.

## Second increment

- **Review finding (Codex, `mtg-engine/tests/leave_game_review.rs`):** a stack ability
  copies its source's `owner`, so the owner sweep removed another player's ability
  when the source's owner left. Abilities aren't owned (CR 113.7a); the sweep now skips
  them. Its control-effect case initially used `layer: 2` instead of
  `layer::CONTROL` (20); reported, and it passes with the right layer.
- **"Each player" / "each opponent" / APNAP:** `GameState::apnap()` lists only players
  still in the game (the active seat still starts the order, CR 800.4j); `Opponents` in
  `eval` and trigger matching use it. A departed player gets no token from "each
  player", isn't counted as an opponent, and their pending triggers are dropped when
  batched (replacing the separate 800.4d filter).
- **800.4e:** combat damage assignments to a departed player are dropped before damage
  is dealt (an unblocked lifelinker whose defender conceded gains nothing).
- **Harness bug:** `scripts/verify_mutations.py` copied sources with their old mtimes,
  so Cargo reused the previous run's last mutant from the shared target dir as the
  "baseline" (the mana off-by-one mutant made commander casting look broken). Copied
  files now get fresh mtimes.
- Evidence: 13 golden tests in `leave_game.rs`; 11/11 production mutants killed;
  22-check PR campaign; 1,872 workspace tests pass (44 ignored, 78 suites); Clippy clean.
- Still open: 800.4b except "each player", 800.4c, 800.4f–h, 800.4m.
