# Commander and Standard pre-alpha candidate — 2026-10-09

User priority: Commander with up to four players, then Standard, then cEDH.
The implementation is ready for local acceptance testing; closed-alpha release
approval remains blocked. The actionable manual checklist is in
[alpha-plan.md](../alpha-plan.md).

Implemented two-to-four-seat local Commander and encrypted hosted pods, private
per-seat views, sequential single-use invites, all-seat scores, concession and
disconnect handling. Multiplayer combat selects different opponents and their
planeswalkers, gathers each defender's blocks in APNAP order, rejects foreign
blocks and associates damage with the actual defender. Added Standard catalog
legality and rejected incomplete card text/layouts before entering matches.
Startup catalog import and missing-image population continue automatically in
the background, with catalog availability before image completion.

## Evidence

- Workspace all-feature tests: 2,123 passed, 44 ignored in 98 suites;
  `/tmp/mtgo-alpha-combat-workspace.log`. The multiplayer combat golden was
  subsequently moved unchanged into mtg-verify and independently passed there.
- Four encrypted loopback hosting tests passed, including a four-player match
  and cancellation of a partially filled lobby. Five session pod tests passed.
- Final all-target/all-feature Clippy, formatting and diff checks passed;
  `/tmp/mtgo-alpha-review-clippy.log`. GUI binary built offline successfully.
- Python checks: 79 tests, one skipped; `/tmp/mtgo-alpha-reviewed-python.log`.
- Fresh PR campaign: 35 checks passed, eight replay fixtures, 17 regression
  fixtures and 800 observed semantic actions;
  `/tmp/mtgo-alpha-final-campaign/report.json`.
- Current-source ordinary gate passed: `/tmp/mtgo-alpha-final-gate.json`.
  Strict release gate failed only the three unknown release assessments:
  `/tmp/mtgo-alpha-closed-alpha-gate.json`.
- Selected real multiplayer mutations: first-opponent-only attacks and foreign
  blockers killed in `/tmp/mtgo-alpha-pod-mutants.json`; first-defender-only blocks
  killed in `/tmp/mtgo-alpha-pod-blocker-mutant.json`. The original third mutant
  compilation error remains in the first report and receives no kill credit.
  Both reports share Rust source digest
  `54b427ee599704431a4d8c76c7fa0f7dffd1d1158f2475fc596961640a1314e3`.
  The full mutation inventory was not rerun; no confidence promotion occurred.
- Permanent independently asserted multiplayer combat regression:
  `tests/regressions/issue_local_multiplayer_combat.json`, generated only after
  the behavioral acceptance test passed. It asserts player life and planeswalker
  loyalty as well as canonical checkpoints.
- Read-only catalog backup coverage: 17,424 / 34,914 (49.9%);
  `/tmp/mtgo-alpha-coverage.log`. This includes a locally added catalog card.
  No new card-coverage work is claimed by this session.

## Remaining acceptance

Independent issue assessments for state corruption, network divergence and
silent unsupported rules remain unassessed in verification.json. Automated
checks cannot substitute for those reviews. Four humans on separate machines,
LAN/internet hosting, Standard best-of-three sideboarding, clean-profile startup
and native installation/update checks remain pending. No external release was
published. CR 613.8 dependency ordering and other documented engine limitations
remain; arbitrary Commander or cEDH decks are not promised to work.

Changes remain uncommitted. Existing deletions of historical root phase/task
documents were preserved and were not performed as part of this work.
