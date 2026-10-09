# Turn-action label audit — 2026-10-09

User reported that the turn button sometimes said “Go to combat” at the wrong
point. The audit found the following presentation errors in format::pass_label:

- Upkeep, draw and beginning of combat shared the first-main “Go to combat” label.
- End of combat, second main and end step shared “End turn,” skipping the
  intervening steps in the description.
- Stack responses said “Let [item] resolve,” although the submitted action is
  one priority pass and opponents may still respond.

Corrected the button's current-step intent: Finish upkeep, Finish draw step,
Go to combat only in first main, Continue combat during combat decisions,
Go to second main at end of combat, Go to end step in second main,
Finish end step and Continue cleanup. Stack/opponent-turn buttons say
Pass priority. Tooltips explain the exact single pass and response window.
The main-phase headings now describe passing toward combat/end step rather
than promising immediate movement or advertising actions that may not be legal.
Stack presence comes from PlayerView.stack even without a resolved card name.

The phase bar previously grouped first-strike damage under “Damage” and cleanup
under “End.” It retains compact slots but displays “First strike” and “Cleanup”
when those steps are current, with exact-step hover text. Button labels use the
pending question's view; Space and button clicks still submit Answer::Pass.
No turn sequencing, priority, auto-pass or other engine behavior changed.

## Evidence

- UI regression drives two actual engine turns, clicks the expected button
  for both players at each priority stop and checks that exactly one pass is
  submitted. It verifies the upkeep/draw/main/combat/end progression was visited.
- Stack-response regression covers both seats, including unnamed stack items;
  phase-bar rendering checks cover first strike and cleanup.
- App library: 244 passed, five ignored;
  `/tmp/mtgo-turn-audit-app-final.log`.
- Formatting/diff checks and all-target app Clippy passed;
  `/tmp/mtgo-turn-audit-clippy-final.log`.
- Fresh PR35 verification campaign and ordinary current-source gate passed:
  `/tmp/mtgo-turn-audit-campaign/report.json` and `/tmp/mtgo-turn-audit-gate.json`.
  Strict alpha issue reviews remain pending as recorded in verification.json.

Prior alpha work was already uncommitted and was preserved. These changes remain
uncommitted. No release was published and no issue assessment was promoted.
