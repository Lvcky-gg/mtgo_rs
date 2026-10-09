# Alpha acceptance runbook

Scope: Commander with two to four players, then Standard. cEDH deck/combination
coverage follows later. This is a runbook for acceptance of the candidate, not
a claim that the candidate has passed closed-alpha release approval. See the
[readiness plan](alpha-plan.md) and [installation guide](releases.md).

Record the app version, operating system/architecture, date, participating
machines and deck lists for each result. Mark each case **PASS**, **FAIL** or
**NOT RUN** and retain reproduction steps for failures. Keep any hidden-card
information private.

## Preparation

Use the same build on every machine. Keep the production profile intact; use
a separate test account/profile for clean-install cases. Let the catalog import
finish, then import the selected lists on the Decks screen. Every card must pass
the support check and format validation before a game starts. Complete text
compilation does not certify interactions against all Magic rules.

The candidate currently supports a single legendary-creature commander.
Multiple commanders and companion designations are refused. Keep those out of
the initial test lists. A deck refused for unsupported cards is an expected
restriction, and should be reported separately from a gameplay failure.

## A. First launch and durable data

1. Launch with a clean test profile and a working network. Confirm import/image
   progress appears and the window remains responsive. Import a deck as soon as
   the catalog is available; the image pass need not finish first.
2. Close during image preparation and restart. Previously valid cached images
   should appear; missing images should continue loading. Check the saved deck.
3. Restart with the network disconnected and an existing catalog. Browse/save
   the deck and start a local game. Missing artwork should have readable text
   fallbacks; downloaded data and settings should remain available.
4. Install the next candidate over the existing one. Confirm decks, settings,
   identity, card catalog and images persist. Run separately on Windows, both
   macOS architectures and Linux Flatpak. Record signing prompts or install
   failures with the exact OS message.

## B. Commander pods

1. Host Commander for four players. Share the displayed invite with one guest
   at a time; after each joins, share the newly displayed invite with the next.
   All guests should wait until the pod is full. Repeat with two and three seats.
2. Verify 40 life, seven-card opening hands and each owner's commander in the
   command zone. One player's mulligan declaration must not redraw their hand
   until everyone still deciding has declared. In a three/four-player pod, keep
   after one mulligan: the hand remains seven. On the second, bottom one before
   deciding again. In a two-player game, the first mulligan bottoms one.
3. Verify the starting player draws on their first turn in three/four-player
   games and skips that draw in a two-player game. Play lands, pay mana and cast
   creatures; let another player respond with an instant or ability.
4. Attack two different opponents and a planeswalker. Each defender should
   declare only their own blocks, in turn order. Compare damage/life/loyalty
   totals across all machines. Track combat damage from each commander separately.
5. Cause an elimination and continue playing. Also concede the host's in-game
   player while the application stays open: the remaining players should finish
   with matching results. Leaving the application/disconnecting ends the hosted
   match; it must not award a false win.
6. Cancel a partly filled lobby. Waiting guests should exit rather than stay
   blocked indefinitely. Rehost and use fresh invites.
7. Run once across separate LAN machines, then across an internet connection
   using the documented tunnel setup. Record which connection method worked.
   Loopback on one machine is a different result.

## C. Standard

1. Import two actual Standard lists whose cards are legal in the refreshed
   catalog and fully supported. Save, edit and reload them. Select Standard
   best-of-three and connect two separate machines.
2. Play a match through sideboarding. Move cards between main deck and sideboard,
   preserve the inventory and keep at least 60 main/at most 15 sideboard cards.
   Confirm the next game uses the changed list and both clients agree on the
   score and final result. Record any known rules limitations affecting the lists.
   Include a supported effect that draws a game: the score stays unchanged,
   sideboarding remains available and a fourth game can start if needed to reach
   two wins. Before each game, only the entitled player should see Play first /
   Draw first, after sideboarding and before their hand. Exercise both choices
   and compare the announced starting player on both machines. After a draw,
   the same chooser decides again, including when they previously drew first.
   Leave once during the prompt and verify the worker/other client exits rather
   than dealing another hand or awarding a match win.
3. Try a catalog-illegal card, including in the sideboard. The match must refuse
   it before beginning. Try an unsupported second face or rules line: it must
   refuse the entire card rather than play partially compiled text.
4. Leave after winning one game of a best-of-three. Both clients should report
   the interruption and the one-game score, without claiming a match winner.

## Reporting a failure

Use **Report a bug** in the app or the contact in the repository README. Include
the build/version, mode and player count, OS/architecture, relevant deck lists,
expected result, actual result and exact steps/decisions. Include screenshots of
the affected board, prompt and score when available. State whether the failure
survives a restart and whether other clients saw the same position.

The UI does not yet export complete live replay attachments automatically.
Offline scenario/replay reports are available through the verification tools;
retain a private copy when supplied. Do not share opponents' hidden cards or
full diagnostic state publicly.

## Release decision

Attach manual results and current automated evidence to the three issue reviews
in `verification.json`. An independent reviewer must assess supported-behavior
state corruption, network divergence and silent unsupported rules; unknowns
remain unknown until reviewed. The strict release gate must pass on the current
source before the candidate is approved for outside alpha testers.
