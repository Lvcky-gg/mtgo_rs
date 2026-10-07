# Commander return choices

Implemented the owner's optional graveyard/exile commander return under CR
903.9a with existing `ChoiceKind::Confirm`/`Answer::Bool`, instead of silently
moving the commander. The existing policy's default remains accept, while players
can decline. Considered object identities prevent repeated prompts; a zone change
creates a fresh eligible identity. Obsolete considered identities are pruned.

Owner decisions are collected in APNAP order before any part of the simultaneous
SBA batch applies. Accepted returns and other mandatory SBAs share the batch.
Pending decisions and their remaining batch are included in canonical diagnostic
snapshots; recording/replay reconstructs them through existing answers.

Independent Rules Lawyer tests reproduced the former forced-return behavior
before edits, then verified both answers in both zones, owner/controller
separation, reentry, APNAP, simultaneity, mandatory deaths before priority and
malformed-answer preservation. All eight Commander golden tests pass. A separate
attack agent reviewed continuation/canonical capture and killed an actual
production mutant that inverted the return decision.

The corpus now contains eight replays and four regressions. The 22-check PR
campaign passes, including 800 semantic actions and 10,000 requested cases per
property. Mutation smoke kills 4/4 compiled mutants after unchanged baselines
pass. Formatting and workspace Clippy pass. No primitive is VERIFIED; closed-alpha
assessments remain unknown. Hand/library commander replacement options and broader
multiplayer cleanup remain outside this increment.

Final all-feature workspace validation passes 1,835 tests across 72 suites, with
44 ignored tests. Network tests used approved local-socket access. Python
automation tests pass (17 tests, one skipped); the final registry tests also pass.
