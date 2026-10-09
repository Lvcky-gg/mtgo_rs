# Pre-alpha acceptance plan

Priority agreed on 2026-10-09: **Commander with two to four players, Standard,
then cEDH**. This replaces the old two-seat assumption; completion percentages
from historical phase summaries are not readiness evidence.

The current branch is a candidate for local acceptance testing. **It is not yet
approved for a closed-alpha release.** The strict gate requires reviewed issue
assessments; `verification.json` still records three unassessed categories. No
rules primitive has been promoted to VERIFIED.

## 1. Commander pods

Implemented and covered by automated acceptance tests:

- Two, three or four local seats; extra local seats are bots using the selected
  bot deck. Commander pods of more than two players run one game.
- Hosting one to three remote players through the existing encrypted transport.
  Every remote seat has a fresh, single-use invite. The host shares the displayed
  invite with one player at a time; the next invite appears after that player joins.
- Decks are checked together before the game begins. Commanders, color identity,
  singleton counts and imported Commander legality are checked. Unknown rules or
  layouts are refused at the local card-source boundary, including second faces,
  sideboard cards and commanders; partial compilation cannot enter a match.
- All four seats receive their own projected views. Only the player assigned a
  question can answer it. Spectators and eliminated players receive updates.
- A player's in-game concession can leave the other players playing, including
  after the host's player concedes. Leaving the application or a broken connection
  ends the hosted match; reconnection is not implemented. A broken connection is
  reported without awarding a false win.
- Attacks can go to different opponents and their planeswalkers. Each defender
  declares only their own blocks, in APNAP order, before priority resumes. Combat
  damage and commander damage remain associated with the attacked player.
- The board already groups multiple opponents; scores now contain all seats.
- Opening hands use declaration rounds: everyone decides before replacement
  hands are dealt. Three/four-player pods grant each player one free mulligan;
  subsequent redraws bottom cards before the next declaration round. Two-player
  games retain the normal paid London mulligan.

Evidence: `crates/mtg-session/tests/pods.rs` checks opening hands, command zones,
turn order, two/three/four-seat results, question routing, private views, host
elimination and disconnects. `crates/mtg-app/tests/network_match.rs` exercises
four encrypted clients, distinct invites and cancellation of a partially filled
lobby. Each encrypted pod seat now exercises its free mulligan.
`crates/mtg-verify/tests/opening_hands.rs` checks declaration order, free/paid
redraws, bottom order, rejection without mutation, the zero-card limit and
first-turn drawing for two/three/four players.
`crates/mtg-verify/tests/multiplayer_combat.rs` checks split attacks, legal
blocker candidates, foreign-block rejection, APNAP order, damage to three players
and a planeswalker, and commander damage. The permanent replay is
`tests/regressions/issue_local_multiplayer_combat.json`.

Combat expectations use [CR 802.2–5](https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt). These are Builder acceptance tests, not independent rules certification.

Before inviting outside testers:

- [ ] Independently review multiplayer seating, projection and disconnect behavior.
- [ ] Play a four-human pod on separate machines, including combat against different
  opponents, commander damage, targeting, an elimination and an in-game concession.
- [ ] Exercise both LAN discovery/invites and internet hosting. Automated loopback
  tests do not establish ngrok availability or internet reachability.

## 2. Standard

Implemented:

- A separate Standard choice in match setup and the deck builder.
- Standard legality comes from the imported catalog, rather than a hard-coded set
  list. Card legality is carried in the match card table and checked by the host.
  Missing Standard legality fails closed; the invented demo deck is not Standard.
- Constructed deck size/copy limits and best-of-three sideboarding still apply.
- Imported Commander legality is also enforced; unrestricted Constructed remains
  a separate option.
- Guests verify game numbering, assigned seating, score transitions and
  sideboarding order. An interrupted best-of-three with a one-game lead has no
  match winner on either client. A snapshot for a different seat is refused
  before it reaches the UI.
- Drawn games do not consume a decisive game in best-of-three. Both clients
  continue sideboarding and game numbering until a player has two wins. Repeated
  draws stop at an explicit 64-game safety limit without awarding a winner.
  Best-of-one still ends after its single game, including a draw.

Evidence: the app's catalog-driven legality regression checks legal/illegal cards
and card-table serialization; existing match/sideboard suites cover structural
limits. Daily startup refresh updates the legality snapshot when online. An offline
catalog is a historical snapshot, not a guarantee of today's rotation or bans.

Encrypted Standard acceptance now imports two synthetic legal decks through the
actual app importer, plays all three games, swaps main/sideboard cards twice,
checks private hands and compares both clients' score, game and sideboarding
events. Both previous losers now exercise drawing first after sideboarding.
A separate encrypted test refuses a catalog-illegal card before either client
begins a game. These loopback fixtures exercise application plumbing;
they do not replace real deck or separate-machine acceptance.

Two-player games now ask the entitled player to play or draw before opening
hands are dealt. The agreed seed selects the first chooser; subsequent losers
choose, and a drawn game retains the previous chooser even if they elected to
draw. Between games the prompt comes after sideboarding. Three/four-player
Commander pods retain their seeded starting seat.

The UI offers Play first / Draw first, clears the prompt when a game starts or
the worker ends, and releases its wait when the player leaves. Guests validate
chooser, score, game number, sideboarding order and their selected starting
player before accepting the game. This follows
[CR 103.1](https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt).
New lobby messages require every client to use this candidate build.

Local and hosted engine regressions resolve an invented simultaneous-life-loss
spell, then play through game four, checking the drawn score, three sideboarding
rounds and the agreed 1–2 result. Protocol tests reject unexplained early endings
and a 65th game, while accepting a completed best-of-one draw.

Before inviting outside testers:

- [x] Implement and automatically verify explicit two-player play/draw selection
  before opening hands, including host/guest agreement and UI cancellation.
- [ ] Import two legal Standard decks that compile completely, then run a full
  best-of-three match with sideboarding on two machines.
- [ ] Verify an illegal card is refused and deck editing/saving preserves the deck.
- [ ] Review whether any accepted card in those decks uses an engine behavior with
  a known rules limitation. Compilation coverage is not a gameplay audit.

## 3. cEDH after the initial alpha

cEDH uses the Commander mode and pod infrastructure; it is not a different rules
format. There is no claim that arbitrary cEDH lists currently work. A copied local
catalog measured 17,424 / 34,914 cards compiling completely (49.9%) on 2026-10-09.
That count includes a locally added card and should not be compared directly with
previous 34,913-card snapshots.

- [ ] Select the actual cEDH deck lists testers want to use.
- [ ] Audit every unique card in those lists; prioritize missing cards by those lists.
- [ ] Add behavioral regressions for their combos, alternate costs, replacement
  interactions and multiplayer wins before expanding the alpha's supported decks.
- [ ] Retain failing games and add reviewed regressions instead of re-recording
  their expected output to hide failures.

## 4. First-launch and release acceptance

Startup automatically downloads/refreshes the card catalog, then checks and fills
the durable image cache in the background. Progress appears on the menu and under
Decks. Images can take substantial time and disk space; cards become available
before all images finish. Missing images fall back to text and retry next launch.

- [ ] Clean-profile launch with networking: catalog appears, progress stays visible,
  deck import works and the window remains responsive.
- [ ] Restart during image population: valid cached files are reused.
- [ ] Offline launch with existing data: decks and games remain usable.
- [ ] Clean-profile install/update smoke tests on Windows, Apple Silicon/Intel macOS
  and Flatpak, including retention of decks, settings and downloaded data.
- [ ] Review all three issue categories in `verification.json` against current
  evidence and record reviewer/evidence provenance and any open defects.
- [ ] Run the strict closed-alpha gate and retain its report. Unknown assessments
  must stay unknown; passing tests alone cannot supply reviewed zero counts.

Local acceptance commands (use new, empty evidence directories):

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
python3 -m unittest discover -s scripts/tests
python3 scripts/verification_campaign.py --tier pr --output /tmp/alpha-campaign
python3 scripts/verification_gate.py --campaign /tmp/alpha-campaign/report.json --output /tmp/alpha-gate.json --release
```

Known rules limitations, including incomplete CR 613.8 dependency ordering, remain
listed in `docs/verification.md`. Review supported deck interactions against those
limitations; do not turn the compiler's complete-text count into a promise of full
Magic rules correctness. Live UI sessions do not yet export complete replay
attachments automatically. For this candidate, bug reports need the build/version,
format/player count, deck lists, reproduction steps and screenshots; use private
channels for hidden-card data. Existing offline scenario reports remain available.

Publication is a separate final step after the above evidence and manual checks.
No release or external invitation is issued by these local changes.

Use the [tester runbook](alpha-testing.md) to collect the remaining manual results.
