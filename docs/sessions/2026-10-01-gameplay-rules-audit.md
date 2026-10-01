# Gameplay rules audit

Checked against Wizards' Comprehensive Rules effective September 25, 2026:
https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt

User specifically reported instant/sorcery behavior and cards that should enter
tapped. The audit reproduced and fixed the following engine problems.

## Trigger observers missed zone changes

Candidate detection examined only the moving card on zone changes, omitting other
permanents that witness land entries, creature entries, and deaths. Restored the
scan of all live observers, keeping the existing last-known-information handling.
CR 603.2 applies to every matching ability, not just the event subject.

Before the fix, three engine detection tests and ten compiled-card tests failed.
Afterwards all pass, including landfall, evolve, graft, Aura death triggers, and
one-or-more death triggers.

## Entering tapped

Face-down permanents returned early from entry processing and therefore ignored
other permanents' effects that make creatures enter tapped. Only their own printed
abilities are now skipped; external replacement effects still apply (CR 708.2,
614.12).

Entry processing emitted ordinary TapChanged events, incorrectly firing
"becomes tapped" abilities on entry. Added an EnteredTapped event with the same
state consequence and distinct trigger semantics (CR 603.2e). Applied it to own
and external entry replacements, effects that put cards onto the battlefield
tapped, and ninjutsu entry. Ordinary tap actions still use TapChanged.

Regressions reproduced both bugs before the changes. Additional end-to-end tests
check creatures and artifacts entering tapped, tapped artifacts having no available
tap mana ability, copies inheriting tapped-entry text, and shock-land life payment
or refusal (including insufficient life).

## Combat damage rules

The engine still implemented obsolete damage assignment ordering. It now gives
priority after blocker declaration (CR 509.2), and allows arbitrary full damage
division among blockers (CR 510.1c). Removed the internal ordering suspension;
the public legacy choice shape is retained. Multiple blockers prompt for positive
damage even when power does not exceed their total toughness. A single blocker
also prompts when trample allows choosing excess damage versus overassignment.
Updated the GUI's damage instructions accordingly.

Trample now reaches the attack destination if all blockers have left combat
(CR 702.19d). Without trample, a previously blocked attacker still deals no damage.
Deathtouch's lethal threshold now accounts for already-lethal marked damage.

First/double strike participation is captured when the first damage step begins.
The second step uses that history and current double strike (CR 510.4), rather
than current first strike. The before-fix regression dealt damage twice after
losing first strike; the paired test also covers gaining first strike without
incorrectly skipping normal damage. Combat cleanup clears the history.

## Casting and resolution checks

End-to-end compiler/engine checks verify instant responses on a nonempty stack,
both spells resolving, instants during combat and on the opponent's turn, and
sorceries requiring the caster's own main phase with an empty stack. These basic
timing paths were correct; no speculative timing change was made. Existing
counterspell, target revalidation, cost, and resolution suites pass.

## Validation and limits

- Full workspace run: 1,186 passed, zero failed, 38 ignored across 58 test binaries,
  including app and multiplayer tests.
- Two subsequent added test cases pass separately: final combat suite has 35
  passing tests, and the entry/casting audit has six passing tests.
- Engine/oracle full suites passed before the final two test additions.
- Core/engine/oracle/app library and binary Clippy with dependency lints disabled
  and warnings denied passes. All-target Clippy fails on pre-existing assertions
  and cloned-reference lints in edge_cases_advanced and trigger_ordering tests;
  mtg-policy also reports unused imports.
- Whitespace diff check passes. No manual desktop game played and no commit made.

This is a focused audit of implemented behavior, not proof of complete rules
coverage. Ignored tests and unsupported compiler/effect shapes remain limitations.
The user's bottom-button hover changes were already uncommitted and preserved.
