# Alpha play/draw selection — 2026-10-09

Two-player games previously forced the seeded player or previous loser to play
first. The match now keeps the entitled chooser separately from the starting
player and asks that seat to choose Play first or Draw first. The seed selects
the first chooser; subsequent losers choose. A drawn game retains its chooser,
even when that player elected to draw. This implements the normal procedure in
[CR 103.1](https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt).

The choice occurs after sideboarding and before the engine deals an opening
hand. Bots default to playing first. Three/four-player Commander pods retain
the seeded starting seat. The new default Seat method preserves existing bot
implementations.

The window receives a match event, presents both buttons without submitting a
default, and sends a boolean answer to the worker. Its pending prompt clears on
game start, match end or worker shutdown. The worker polls its cancellation flag
every 100ms while waiting; leaving or dropping the response channel releases it.
The local/host loop checks leaving before dealing and can end with a zero score
without awarding a winner. The sideboard button now says Done — continue.

New PlayDraw lobby variants announce the chooser/game/score and carry the
remote chooser's reply. Guests reject duplicate, stale or malformed requests,
the wrong chooser, a request before required sideboarding, games without prior
agreement and a starting player contrary to their answer. The host rejects a
reply for another game. All clients must use this candidate build; this does
not add cross-version protocol negotiation.

## Evidence

- Session library: 35 tests passed. New regressions cover both initial choosers
  electing to draw, retained choice after a real engine draw, subsequent losers,
  host/guest draw-first agreement, invalid requests/replies and leaving during
  the initial choice without starting another game.
- UI tests click both actual egui buttons, check pending-prompt cleanup and
  exercise answer/leave/channel-close release of the UiSeat worker wait.
- All six encrypted network acceptance tests passed. The imported synthetic
  Standard match sideboards twice and both previous losers elect to draw;
  clients agree on starts, score and the 1–2 result. These fixture decks are
  not evidence that arbitrary real Standard decks are supported.
- Workspace/all-feature suite: **2,154 passed, 44 ignored**, 99 suites;
  `/tmp/mtgo-alpha-playdraw-workspace.log`.
- All-workspace/all-target/all-feature Clippy, formatting and diff checks passed;
  `/tmp/mtgo-alpha-playdraw-clippy-final.log`.
- Fresh PR campaign: **35 checks passed**;
  `/tmp/mtgo-alpha-playdraw-campaign/report.json`.
- Ordinary current-source gate passed; `/tmp/mtgo-alpha-playdraw-gate.json`.
  Source fingerprint:
  `3aee9341e95f0d68c977b3c466e1aaf674d85784f5f3736aaa415ec42d07ff6f`.

Strict release gate still fails only for unassessed state_corruption,
network_divergence and silent_unsupported_rules;
`/tmp/mtgo-alpha-playdraw-release-gate.json`. Independent review, real-deck and
separate-machine acceptance remain pending. No confidence promotion or release
occurred. The alpha plan/runbook now reflects the implemented choice flow and
supersedes the play/draw limitation recorded in the drawn-match session.
Prior uncommitted work was preserved; these changes remain uncommitted.
