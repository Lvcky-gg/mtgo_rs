# Alpha opening hands and match consistency — 2026-10-09

Continued the Commander/Standard alpha plan after the hub work. The worktree
was clean at entry; the preceding implementation and website were already
committed. Changes from this session remain uncommitted.

## Defects fixed

The pregame previously let one player redraw and decide repeatedly before
other players declared, bottomed only after a keep, and charged for multiplayer's
first mulligan. It now collects declarations in starting-player/turn order,
redraws after the round, bottoms before the next declaration, excludes kept
players from subsequent rounds and grants each three/four-player seat one free
mulligan. The zero-card limit counts only paid mulligans. The UI explains the
free redraw and displays the correct resulting hand size. Two-player Commander
uses the ordinary paid London mulligan.

Expectations derive from [CR 103.5 and 103.5c](https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt).
The rules also distinguish first-turn draws in two-player and multiplayer games
under CR 103.8a/c; the acceptance test covers the existing correct behavior.

Guests previously picked the sole score leader as match winner even when the
host ended a best-of-three early with no match winner. Guests now apply the same
wins-needed rule as the host and verify final scores against observed games.
They reject changed seats, nonsequential game numbers, forged score transitions,
omitted/unrequested sideboarding, unsupported multiplayer formats and invalid
match lengths before those messages reach the UI. Guest snapshots are pinned to
the lobby's assigned seat, even before the first decision.

## Acceptance evidence

- Workspace: 2,135 passed, 44 ignored in 99 suites;
  `/tmp/mtgo-alpha-opening-workspace.log`, exit 0.
- Formatting, diff checks and all-target/all-feature Clippy passed;
  `/tmp/mtgo-alpha-opening-final-clippy.log`, exit 0.
- Python: 79 tests, one skip; `/tmp/mtgo-alpha-opening-python.log`.
- Fresh PR35 campaign: eight replays, 17 regressions, 800 observed semantic
  actions; `/tmp/mtgo-alpha-opening-campaign/report.json`, passed.
- Ordinary current-source gate passed: `/tmp/mtgo-alpha-opening-gate.json`.
  Strict release rejected exactly the three unknown issue assessments:
  `/tmp/mtgo-alpha-opening-release-gate.json`.
- Three selected compiled production mutants killed: charged first multiplayer
  mulligan, premature redraw and skipped bottom choice;
  `/tmp/mtgo-alpha-opening-mutants.json`. Rust digest:
  `bd015218e0aecb5884310675780ddb9fe90c280fadd7e7c6da00bf0c447d1a86`.
  This snapshot precedes the last additions to encrypted-network tests; the
  production mulligan implementation and opening-hand golden are unchanged.
  The full mutation inventory was not rerun. No confidence promotion occurred.

Four opening-hand goldens in mtg-verify cover two/three/four players, rotated
starting seats, deferred redraws, exclusion of kept players, free/paid mulligans,
ordered bottoming, canonical rejection without mutation, eight multiplayer
mulligans to zero and first-turn drawing. Existing engine mulligan tests now
exercise the corrected declaration/bottoming sequence.

Six additional session checks cover malformed protocol transitions and a real
host/guest match interrupted after game one. Encrypted app acceptance now imports
synthetic legal Standard decks through the actual importer, plays three games
with controlled concessions, swaps main/sideboard cards twice and compares both
clients' events/results. A separate encrypted match refuses a catalog-illegal
card before either seat begins. The four-client Commander test makes every seat
take and keep its free mulligan, checking private projections along the way.

An initial app-library run in the sandbox passed 230 tests but failed 11 socket
tests at bind with PermissionDenied. The authorized rerun with sockets passed;
the original failure log remains `/tmp/mtgo-alpha-opening-focused.log`.

## Remaining readiness

The [tester runbook](../alpha-testing.md) makes the remaining manual acceptance
steps explicit. Separate-machine humans, internet hosting, actual Standard lists,
clean-profile launch/install/update on supported OSes and independent issue
reviews remain pending. No manual cases were marked passed from loopback tests.
verification.json remains unassessed; no release was published.
