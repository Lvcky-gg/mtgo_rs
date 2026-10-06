# mtgo_rs

A Magic client in Rust: native app, offline by design, peer-to-peer over
WebSockets, local database, and a rules engine that resolves triggers without
interrogating you about every one of them.

Full architecture and the reasoning behind each decision:
`~/Documents/obsidian/Brain/03_Projects/mtgo-rs/mtgo-rs-architecture.md`

## Status

The engine plays a turn. Turn structure, priority passing, state-based actions, the
layer system, the evaluator and trigger placement all work and are tested. What is
missing is listed honestly below.

```sh
cargo run -p mtg-headless              # play 3 turns in the terminal
cargo run -p mtg-headless -- --turns 8 # longer
cargo run -p mtg-headless -- -v        # every event, not just steps
cargo run -p mtg-headless -- --stops   # auto-pass OFF, to see what it saves you

cargo run -p mtg-app --bin mtg-gui      # play against a policy bot in a window
cargo test --workspace
cargo test --workspace --all-features  # includes transport integration tests
cargo clippy --workspace --all-targets
```

The arena includes short gameplay sounds with **Mute**, **Volume**, and **Test
sound** controls. Settings carry between matches while the app stays open. Linux
builds need ALSA development headers and pkg-config (on Debian/Ubuntu:
`sudo apt install libasound2-dev pkg-config`). Audio-device failure leaves the game
usable without sound. The original cues and generator are in [assets/sounds](assets/sounds/README.md).

The deck builder suggests cards from shared rules and type themes, filters suggestions
by format and color identity, suggests roles missing from the main deck, and shows
estimated roles, mana curve, average spell
cost, and colored mana demand versus potential land sources. These are text-based
estimates. A suggestion focus selector offers balanced, synergy, missing-role,
and early-curve rankings. Analysis includes exact opening-seven land probabilities
for the current main deck before mulligans. Import accepts pasted lists or dropped UTF-8 files; export supports plain
text, Arena, MTGO, Moxfield, Archidekt, CSV, and JSON. Spreadsheet imports accept
comma-separated or tab-separated columns. Structured imports use CSV
columns `count,name,section` or a JSON `entries` array of objects with those fields.

Tournament suggestion mode downloads official MTGO Modern event lists or accepts
published Modern lists with event metadata,
keeps an offline dataset, and ranks additions from similar recent lists. It shows
sample counts, typical copies, sources, and a comparison with tournament mana
curves. Deck and suggestion rows have clickable card previews. Saved deck profiles
cover self-rated power, competitiveness, Commander bracket, and Rule 0 expectations.
See [tournament data and deck profiles](docs/tournament-data.md) for imports and
how the evidence and declarations are interpreted.

`mtg-play` is the reference driver: it consumes exactly what a GUI would — the legal
action list and the choice stream — so anything it can do, the real app can do.

It also puts a number on the auto-pass design. One turn, same game:

| | prompts a human sees |
|---|---|
| `--stops` (auto-pass off, roughly MTGO's default) | **21** |
| default (auto-pass on) | **1** |

The one remaining prompt is a real decision — which land to play.

**Working:** turn steps and turn-based actions · priority passing · the CR 117.5
settle loop (state-based actions to a fixpoint, then triggers on the stack) · the
CR 613 layer system including sublayer ordering · selector/value/condition/filter
evaluation · event application and replay-grade logging · effect resolution for the
choice-free primitives · combat damage · trigger ordering analysis end to end.

**Working** also: mana availability and cost payment — casting auto-taps, spending floating mana
first and then the least flexible sources, and lands can be tapped by hand to float mana · cost modifiers · casting ·
the stack (LIFO, one object per round of passes) · combat — attack and block
declaration, flying/reach/menace/defender/vigilance, first and double strike,
deathtouch, trample, blocker ordering, damage assignment.

**Working** also: trigger detection — event-pattern matching, last-known information
for dies triggers, intervening-if clauses, once-per-turn limits, and event bindings
so "it" refers to the right thing.

**Working** also: choices gathered during resolution — optional effects, modes,
discard, sacrifice, scry-like sorting, and player-chosen numbers.

**Working** also: targeting — legality on announcement, hexproof/shroud, distinct
target slots, re-checking on resolution, and fizzling when every target is gone.

**Working** also: countering, including "can't be countered", counterspells countering
each other, and a counterspell fizzling when its target already resolved.

**Working** also: triggered abilities that target — placed one at a time so each can
choose its targets as it goes on the stack (CR 603.3d).

**Working** also: state triggers (CR 603.8) — polled alongside state-based actions, with
the CR 603.8 guard that stops a still-true condition re-firing forever.

**Working** also: undo (Ctrl+Z in the window). A player can take back their own priority actions
— a mana tap, a land, a cast — while nobody else has been asked anything since and nothing hidden
or random has happened. It is an ordinary answer (`Answer::Undo`), so it works over the wire too.

**Paid mana abilities:** filter sources and life-paying sources can be activated manually;
input mana and life are paid before output is produced. Automatic tapping still uses
sources without mana or life activation costs, so float mana from paid sources first.
Fixed mixed-colour output (such as `{W}{U}`) is supported too.

**Planeswalkers:** fixed positive, negative and zero loyalty abilities pay during activation,
use the stack, and share one activation per permanent per turn at sorcery timing. Fixed
mana-producing loyalty abilities also use the stack. Creatures can attack a planeswalker
or its controller; choose each attacker's destination in the attack panel. The destination
appears on the board and in the log. Combat damage removes loyalty, with blocking, trample,
lifelink and damage triggers applied normally. Variable loyalty costs and colour-choice
mana output from loyalty abilities remain unsupported by the compiler.

**Combat damage prevention:** “Prevent all combat damage that would be dealt this turn”
prevents both combat-damage steps, including damage to creatures and planeswalkers.
Prevented damage grants no lifelink, poison, or damage triggers. Spell damage still
works, and prevention expires at cleanup. The client shows its active status; the bot
holds pure prevention spells for an opponent's declared attack.

**Targeted prevention:** “Prevent all damage that would be dealt to target creature
this turn” also supports target players and any target. It prevents combat and spell
damage to that recipient, expires at cleanup, and does not follow a card after a zone
change. The client names protected recipients, and the bot prefers friendly targets.

**Not yet, rules side:** CR 613.8 layer dependency ordering · copy effects (CR 707) ·
additional costs paid mid-resolution other than mana · granted non-keyword abilities ·
fuse split casting · transforming cards · numeric and source-specific damage prevention.

**Card import and persistence work:** downloads stream into SQLite with byte/card progress
and indexed updates. Existing databases migrate automatically when opened by this build.

```sh
cargo run -p mtg-headless --bin mtg-cards -- import <bulk-file.json>
cargo run -p mtg-headless --bin mtg-cards --features fetch -- import --fetch
cargo run -p mtg-headless --bin mtg-cards -- search lifelink trample
cargo run -p mtg-headless --bin mtg-cards -- deck "My Deck" list.txt
cargo run -p mtg-headless --bin mtg-cards -- stats
```

**Rules text compiles (ADR-013).** `mtg_oracle::compile` reads each rules-text line through a
compositional grammar — targets, damage, pumps, triggers, costs, tokens, statics, conditions,
modes, X — and emits IR, or nothing: a line with any word the grammar does not know compiles to
nothing, and only shapes the engine carries out are emitted. A card is *playable as printed* when
every line of every face compiled and its layout is one the engine plays (single-faced, modal
double-faced, adventure, or supported split). MDFCs offer each legal spell or land face separately and retain the chosen face
on the stack and battlefield. Adventures exile after successful resolution and let the
resolving player cast the normal card from exile, with ordinary timing and costs.
Plain split cards offer either half with its own cost, timing, and targets. Their characteristics
combine outside the stack; on the stack only the chosen half applies. Aftermath halves
can be cast only from a graveyard and are exiled whenever they leave the stack. Fuse,
split permanents, and split abilities that require execution outside the stack remain excluded.

```sh
cargo run -p mtg-headless --bin mtg-cards -- coverage 40         # the number, and what to teach next
cargo run -p mtg-headless --bin mtg-cards -- coverage 0 --list   # every accepted card, for review
cargo run -p mtg-headless --bin mtg-cards -- parse Instant "Draw two cards."
```

Currently **8,598 of 34,898 cards (24.6%)** are playable as printed, up from 1,258. Every shape the
compiler emits has an end-to-end test in `crates/mtg-oracle/tests/compiled/` that takes printed
text through import and plays it in the engine.

**The session protocol works offline.** Identity (real Ed25519, persisted),
signed single-use invite links, commit-reveal shuffle fairness with a verifiable shuffle,
and the host/guest handshake are built and tested — 53 tests, all without a network, because
the protocol is a pure function over messages.

**The transport works.** WebSocket + Noise encryption with Ed25519 channel binding, and an
end-to-end test where two peers connect over a real loopback socket, authenticate each other,
and agree on a shuffle neither side controls:

```sh
cargo test -p mtg-net --features transport
```

**A complete game plays between two peers over an encrypted socket.** `mtg-session` joins the
rules to the wire: the host drives the `Engine` and projects a `PlayerView` per question, the
guest answers and holds nothing but the latest view.

```sh
cargo test -p mtg-session --features transport
```

That test binds a port, issues an invite, parses the pasted link, connects, encrypts,
authenticates, agrees a shuffle neither side chose, then plays a game to its natural end with
both sides agreeing who won.

`PlayerView` is the only state type in the engine that derives `Serialize` — so the transport
*cannot* send a `GameState`, and a hidden-information leak is a compile error. There is also a
test that inspects every view the guest received and asserts no library card or opponent hand
contents appear.

**There is a window.** `mtg-gui` (egui, see ADR-009) plays you against a simple bot on the demo
cards (`mtg_policy::bot`): it plays lands, uses loyalty abilities, casts its most expensive spell, attacks where no blocker
can punish it, and blocks to kill or to survive. It prioritises lethal player damage,
otherwise pressures planeswalkers, and blocks to preserve a threatened planeswalker
when its own life is safe. Simple harmful effects target opponents; helpful draw and
life effects target its own seat. Like the UI it holds no rules — it picks from the
engine's legal actions and its own seat's view. The policy answers first and only real decisions reach you, with the count of
what it handled shown in the status bar. It has controls for priority, attacking, blocking, targets,
modes, X, object choices and the legend rule; the remaining choice kinds (trigger/replacement
ordering, mana payment, damage assignment, blocker ordering) take the engine's legal default and
say so.

**Cards are drawn as their images.** The first time a card is seen, its image comes from Scryfall's
CDN — using the URL recorded at import, or the by-name endpoint for older imports — and is cached
under `~/.cache/mtgo_rs/images/`. After that, play works offline. An invented card has no printing,
so it stays a text panel unless your local `~/.config/mtgo_rs/art.txt` maps it
(`Training Field = Plains`). That file is yours; the repository ships no card names. Mana symbols in costs, rules text and
menus are Scryfall's SVGs, cached the same way, with a plain drawn disc until they arrive.

**The window is a full client.** `mtg-gui` opens on a menu: **Play vs Bot**, **Host a game**,
**Join a game**, and **Decks**. Decks are pasted in (Moxfield, Archidekt, Arena, MTGO or plain
text) and saved locally, and the card database downloads from Scryfall with one button. A match
is Constructed or **Commander**, and best of 1 or **best of 3** with sideboarding. Hosting shows
an invite link to send; joining takes that link or selects a discovered nearby game.
Choose **Nearby players** for automatic LAN discovery, or **Friends online** to open an
embedded ngrok HTTPS tunnel. Only the host needs an ngrok account and authtoken; no separate
ngrok install, local address entry, or port forwarding is needed. See [hosting setup](docs/hosting.md)
and ADR-012.

**Imported decks play.** Every card's rules text goes through the compiler on import, and the deck
screens show how much of each deck is fully playable as printed.

**Deck building and testing.** The Decks screen has a builder: search every card (name, type,
rules text, colours, and "legal in" a format — which also hides joke and digital-only cards),
click to add, right-click for sideboard or commander, hover for the card large. The deck sits
beside the results, grouped by type with a mana curve, legality for Constructed or Commander,
**Sample hand**, **Copy list** (the same text Import reads), and **Playtest**, which plays the
deck against an opponent who does nothing.

**Playing.** London mulligans; discard to hand size; lifelink. The board is answered by clicking
it — attackers, blockers (click yours, then theirs), targets, cards to bottom or discard — and
shows who is attacking and blocking. A phase bar, a Pass button that says where it goes (Space),
and a log narrated from what changed ("Opponent attacked with…", "You lose 3 life (17)"),
including everything that happened while auto-pass was handling your priority.

**Still unbuilt:** a self-hosted relay,
redacted event streaming instead of whole snapshots, and reconnect exercised rather than designed.

Unimplemented primitives return a named error rather than a silent no-op, so a gap
shows up in a failure message instead of looking like working code.

## Crate map

| Crate | Responsibility |
|---|---|
| `mtg-core` | Vocabulary: ids, zones, characteristics, mana, the event enum. No logic. |
| `mtg-engine` internals | `eval` (selectors/values/filters) · `layers` (CR 613) · `apply` (the only mutator) · `sba` (CR 704) · `turn` · `resolve` · `engine` (the driver) |
| `mtg-ir` | Cards as **data**: ability/effect/trigger trees, plus the read-write analysis that decides whether a trigger ordering prompt is necessary. |
| `mtg-engine` | Pure state machine: turn structure, priority, stack, state-based actions, the layer system, trigger pipeline, per-player view projection. |
| `mtg-cards` | The card data files and the native escape-hatch registry. |
| `mtg-oracle` | Imports card data from Scryfall at first run. Ships no card data. |
| `mtg-deck` | Paste-in deck import: Moxfield, Archidekt, Arena, MTGO, plain text. |
| `mtg-store` | SQLite: decks, match history, replays, preferences, identity key. |
| `mtg-net` | Invite tokens, identity, transport, commit-reveal shuffle fairness. |
| `mtg-policy` | Client-side answering policy: auto-pass, choice elision, stops. Not rules. |
| `mtg-headless` | Scriptable frontend for tests and AI. |
| `mtg-app` | The native application. |

## The four decisions that shape everything

1. **Cards are data, not code.** A card is a tree of primitives in a file. This is
   what makes the trigger analysis below possible, and it means adding a card
   ships no binary.

2. **The engine is a pure function.** No I/O, no clock, no ambient randomness —
   seeds arrive inside commands. Replay, reconnect and post-game verification all
   fall out of this rather than being built.

3. **Ordering prompts are proven unnecessary, not skipped.** CR 603.3b makes you
   order simultaneous triggers. Usually the order cannot matter, and being asked
   anyway is the single most tiring thing about playing Magic on a computer.
   `mtg-ir::footprint` derives what each trigger reads and writes; when no two
   overlap, every order gives the same result, so the engine picks one and says
   nothing. When they do overlap you are asked — and told which shared thing makes
   it matter. A wrong auto-order would be a rules violation, so the analysis is
   conservative in exactly one direction: it asks when unsure.

4. **The UI holds no rules.** The engine publishes the set of legal actions; the
   view renders that list. Nothing is clickable that is not in it.

5. **Resolution is restartable, not suspendable.** An effect tree is a recursive walk
   that applies events as it goes, so there is no resume point to store. Instead the
   pre-resolution state is kept, and a needed choice rolls back, asks, and re-runs the
   whole resolution with the answer appended — sound because resolution is deterministic
   given its start and its answers. Rollback happens before the question is handed out,
   so nothing half-applied is ever observable, and nothing is applied twice.

6. **Triggers are detected by scanning the log, not by a hook in event application.**
   A replayed log therefore detects exactly the same triggers — detection is a pure
   function of the event stream, so it inherits replay and reconnect correctness rather
   than needing its own. Scanning after the fact is safe because CR 603.3 makes a
   trigger wait for the next grant of priority anyway.

7. **Mana availability is a matching, not a count.** A source that makes one of
   several colours gets double-counted by any per-colour total: one "white or green"
   land looks like it covers `{W}{G}`, but it makes one mana. Coloured symbols are
   matched to *distinct* units of producible mana with augmenting paths. The same
   function answers "can I cast this?" and "how do I pay?", so the two can never
   disagree.

## The driver contract

```rust
loop {
    match engine.advance(cards) {
        Progress::Continue       => continue,
        Progress::NeedsChoice(c) => engine.answer(c.id, policy_or_human(c))?,
        Progress::GameOver{..}   => break,
    }
}
```

`advance` never blocks and never asks — it either makes progress or hands back a
question. An outstanding choice is re-emitted rather than skipped, which makes
`advance` idempotent and is why a reconnecting peer needs no special path.

## Ships no card data

No card names, oracle text, images or set data are in this repository. `mtg-oracle`
fetches Scryfall's bulk data on first run into the local database, and card images
are cached from their CDN. Cards in the tests and docs are invented.

This is also why new sets work without a release.

## Scope

"Triggers for all cards" is the goal, not a milestone. There are roughly thirty
thousand unique cards, and a rules engine that handles them all is years of work —
any plan that claims otherwise is wrong about the problem.

What is achievable is *measurable* coverage. The ~250 effect primitives in
`mtg-ir` cover most of a given format's card pool, so the build order is by format
(a cube, then Pauper, then one Standard set), and CI reports the percentage of each
format's cards that the IR fully expresses. Coverage going up is the metric;
`AbilityKind::Native` count going up is the warning sign.

## License

AGPL-3.0-or-later.
