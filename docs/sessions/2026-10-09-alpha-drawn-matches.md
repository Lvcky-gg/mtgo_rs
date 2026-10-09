# Alpha drawn-match progression — 2026-10-09

The match loop previously stopped after three games in best-of-three even if
one or more games were drawn. A 1–1 score could therefore end without either
player reaching two wins; the guest also refused game four and stopped asking
for sideboarding after game three.

Best-of-three now continues until two wins. Draws leave the score unchanged,
retain the previous starting player and allow another sideboarding round.
Best-of-one still ends after exactly one game, including a draw. Host and guest
stop at a 64-game operational limit with an explicit reason and no false match
winner. The host sends the same end reason it returns locally. Guests reject an
unexplained early best-of-three ending even when its final score is unchanged.
No lobby message variants or wire layouts changed.

The current alpha still defaults to the previous loser playing first. Explicit
play/draw selection is pending and is recorded in the acceptance plan/runbook;
this change does not claim full tournament procedure support.

## Validation

Six new regressions cover:

- Local and hosted engine matches: an invented free spell makes both players
  lose life simultaneously in game one. Both seats then play three decisive
  games, sideboard three times and agree on the four-game 1–2 result. The draw
  uses real engine resolution; the Basic fixture cards are invented and do not
  establish real-deck legality or rules certification.
- Repeated-draw score termination and the unchanged best-of-one behavior.
- Guest acceptance of 64 drawn games and rejection of a 65th before UI delivery.
- Rejection of unexplained early endings after either a draw or a one-game lead.
- Acceptance of a completed best-of-one draw without an error reason.

Full workspace/all-feature tests: **2,144 passed, 44 ignored**, 99 suites;
`/tmp/mtgo-alpha-draw-workspace.log`. All-workspace/all-target/all-feature Clippy,
formatting and diff checks passed; `/tmp/mtgo-alpha-draw-clippy.log`.

Fresh PR verification: **35 checks passed**;
`/tmp/mtgo-alpha-draw-campaign/report.json`. Ordinary current-source gate passed;
`/tmp/mtgo-alpha-draw-gate.json`. Source fingerprint:
`d572e566feea3abf7bc45ee8dd4c70a14393c7b36566f25e2d24f03c6429aad6`.

Strict release gate failed only for the three unassessed issue categories:
state_corruption, network_divergence and silent_unsupported_rules;
`/tmp/mtgo-alpha-draw-release-gate.json`. Independent review and human acceptance
on separate machines remain pending. No confidence promotion or release occurred.
Prior uncommitted alpha work was preserved; these changes remain uncommitted.
