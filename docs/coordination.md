# Current work

Codex (2026-10-07, Commander return choices): owner can accept/decline graveyard
and exile returns once per new object, with APNAP decisions and simultaneous SBA
application. Eight independent Commander tests, eight replays/four regressions,
22-check PR campaign and four production mutants pass. See
[session](sessions/2026-10-07-commander-return-choices.md).

Codex (2026-10-07, Commander verification): shared Command representation fixed
across casting/returns/monarch/session setup, with owner-only casting eligibility.
Four independent golden tests, Commander replay and permanent regression added;
20-check PR campaign and three compiled production mutants pass. Optional owner
return decisions remain unverified. See
[session](sessions/2026-10-07-commander-verification.md).

Codex (2026-10-07, verification foundation): new `mtg-verify` scenario/replay,
canonical/property/fuzz/differential/mutation/confidence and CI tooling. Independent
campaign found and fixed eliminated-player priority/turn rotation; six replays,
two regressions and final 18-check PR campaign pass. Closed-alpha assessments
remain unknown and no primitive is VERIFIED. See
[session and remaining scope](sessions/2026-10-07-agent-first-verification.md).

Claude (2026-10-07, card coverage round 58 — COMPLETE): 17,154 → 17,165 / 34,913
(49.2%); all 12 faces mine; zero lost against HEAD.
- The old Oblivion Ring wording: an enters trigger's "exile …" plus "When ~ leaves the
  battlefield, return the exiled card(s) …" → `Effect::ExileLinked` (remembered in
  `state.exiled_with`) and `Effect::ReturnExiledWith`, paired by a post-pass in
  `compile()`. Not "exile until ~ leaves": if the source is gone before the enters trigger
  resolves, the card stays exiled (CR 610.3 doesn't apply to the two-ability version).
  The leaves trigger's source is the card in the graveyard; the return follows the log's
  zone changes back to the permanent that exiled.
- A first version also caught Voyager Staff's delayed "return the exiled card" and lost it;
  the return is now read only as the whole body of a self leaves trigger.
- Tests: `activation_limits.rs` (+1).

Claude (2026-10-07, card coverage round 57 — COMPLETE): 17,140 → 17,154 / 34,913
(49.1%); all 14 faces mine; zero lost against HEAD.
- Tarmogoyf's CDA: "~'s power is equal to <v> and its toughness is equal to that number
  [plus N]" → `SetBasePowerToughness`.
- `Restriction::CantBlockSource` — "target creature can't block ~ this turn" (checked in
  `combat::can_block`).
- Tests: `value_phrases.rs` (+2).

Claude (2026-10-07, card coverage round 56 — COMPLETE): 17,120 → 17,140 / 34,913
(49.1%); all 20 faces mine; zero lost against HEAD.
- `Duration::WhileSourceTapped` — "for as long as ~ remains tapped" on pumps and the
  tappers' "it doesn't untap during its controller's untap step for as long as ~ remains
  tapped"; ends as the source untaps (`apply`, `TapChanged`) or leaves.
- "Roll a six-sided die" (worded die sizes).
- Tests: `activation_limits.rs` (+1).

Claude (2026-10-07, card coverage round 55 — COMPLETE): 17,117 → 17,120 / 34,913
(49.0%); 3 faces; zero lost against HEAD.
- "<player> exiles a <noun> they control" → `AsPlayer` + a chosen `MoveZone` to exile.
- Probed every player-subject sentence in not-understood lines (468 → 399 failing):
  the rest is a long tail (mass sacrifice/return "all … they control", reveal follow-ups).
- Tests: `its_controller_may.rs` (+1).

Claude (2026-10-07, card coverage round 54 — COMPLETE): 17,058 → 17,117 / 34,913
(49.0%); all 62 faces mine; zero lost against HEAD.
- "<player> creates <tokens>" (any non-"you" `nouns::player` subject: "its controller",
  "target opponent", "each player", "that player" …) → `CreateToken { controller }`:
  Beast Within, Generous Gift, Crib Swap, An Offer You Can't Refuse, Afterlife.
- Mass recipients "each creature and (each) planeswalker [you/your opponents/you don't
  control]", "each creature, planeswalker, and battle".
- Found by probing the bullets of modal blocks that are a card's only obstacle (412
  distinct failing bullets — mostly long tail).
- Tests: `create_number.rs` (+2).

Claude (2026-10-07, card coverage round 53 — COMPLETE): 17,031 → 17,058 / 34,913
(48.9%); all 27 faces mine; zero lost against HEAD.
- Planeswalker-deck searches: "you may search your library and/or graveyard for a card
  named <name>, reveal it, and put it into your hand. If you search your library this way,
  shuffle." → new `Effect::SearchLibraryAndGraveyard { filter }` (one optional card from
  either zone; shuffle unless it came from the graveyard). **For Codex (UI):** a
  ChooseObjects mixing library and graveyard cards.
- Tests: `new_triggers.rs` (+1).

Claude (2026-10-07, card coverage round 52 — COMPLETE): 17,003 → 17,031 / 34,913
(48.8%); all 28 faces mine; zero lost against HEAD.
- "create <amount> <tokens>" for any non-fixed `amount()` — X (when defined) and "that
  many" (the triggering event's amount): Hornet Nest, Old Gnawbone, Tana.
- "double the number of <kind> counters on <one object>" → `AddCounters` of its own count:
  Primordial Hydra, Growth Curve, Solarion.
- Tests: `create_number.rs` (+1).

Claude (2026-10-07, card coverage round 51 — COMPLETE): 16,991 → 17,003 / 34,913
(48.7%); all 13 faces mine; zero lost against HEAD.
- Spell filters `ObjectFilter::CastFromZone`, `Kicked`, `HasXInCost` (eval and detect);
  triggers "you cast a kicked spell", "… a spell from your graveyard", "… a spell from
  anywhere other than your hand", "… a spell with {X} in its mana cost".
- Tests: `new_triggers.rs` (+1: kicked triggers, unkicked doesn't).

Claude (2026-10-07, card coverage round 50 — COMPLETE): 16,970 → 16,991 / 34,913
(48.7%); all 22 faces mine; zero lost against HEAD.
- Conditions: coven ("you control three or more creatures with different powers", new
  `Value::DistinctPowers`), "this spell was cast from a graveyard", "~ is on the
  battlefield"; the "you control …" branch no longer returns early.
- Values: "your speed" (`Value::Speed`), "half your starting life total" (no rounding
  needed), "that permanent's …".
- **Engine fix:** `Condition::WasCast` read only the permanent-side `cast_from`, so a spell
  still on the stack never knew where it was cast from ("if this spell was cast from a
  graveyard", "if you cast it from your hand" on spells); it now reads the cast context.
- Tests: `value_phrases.rs` (+2).

Claude (2026-10-07, card coverage round 49 — COMPLETE): 16,936 → 16,970 / 34,913
(48.6%); all 34 faces mine; zero lost against HEAD.
- Activated-ability costs: "return <n> <noun> you control to its owner's hand" (Codex's
  `ReturnToHand`, now also in `cost()`), "return ~ to its owner's hand"
  (`ReturnToHand { IsSelf }`), "mill N cards" (new `AdditionalCost::Mill`, payable only
  with that many in the library), "put N <kind> counters on ~" (new
  `AdditionalCost::PutCounters`).
- Tests: `activation_limits.rs` (+2).
Handoff: `docs/sessions/2026-10-07-exhaust-boast-exile-cost.md` (updated).

Claude (2026-10-07, card coverage round 48 — COMPLETE): 16,895 → 16,936 / 34,913
(48.5%); all 41 faces mine; zero lost against HEAD.
- Costs (probed as "<cost>: Draw a card."): "Exile this artifact / ~" as a cost
  (`ExileFrom { zone: Battlefield, filter: IsSelf }`, paid like the graveyard one).
- Exhaust (CR 702.177): `ActivationTiming::Exhaust`, `state.activated_ever`.
- Boast (CR 702.142): once each turn, with a cost condition that it attacked this turn.
- Not done: Power-up (rules unverified), snow mana {S}, waterbend, discard at random.
- Tests: `activation_limits.rs` (3, new).
Handoff: `docs/sessions/2026-10-07-exhaust-boast-exile-cost.md`.

Claude (2026-10-07, card coverage round 47 — COMPLETE): 16,855 → 16,895 / 34,913
(48.4%); all 44 faces mine; zero lost against HEAD.
- Conditions (probed as "As long as <condition>, …"): "~ is an enchantment", "you gained
  N or more life this turn", "you have a full party", "X is N or more" (when X is defined),
  "there are N or more <noun> cards in your graveyard" (spell mastery etc.); the "you
  have …" branch no longer returns early without a number.
- **Fix:** "create X Soldiers … destroy all other creatures" destroyed the new tokens
  ("other" only excluded the source). `Cx::made_tokens` makes "other" also exclude the
  tokens just made (`InBinding(It)`) — found by a test of Martial Coup.
- Tests: `create_number.rs` (+1), `value_phrases.rs` (+1).
Handoff: `docs/sessions/2026-10-07-conditions-and-other-tokens.md`.

Claude (2026-10-07, card coverage round 46 — COMPLETE): 16,841 → 16,855 / 34,913
(48.3%); all 14 faces mine; zero lost against HEAD.
- `value_phrase` wraps `value_phrase_base` for "<n> minus <v>" and "<v> minus <n>"
  (negative results deal/gain nothing — tested), and reads "the number of creatures in
  your party". Iron Maiden, Viseling, Storm World, the party cycle.
- Tests: `value_phrases.rs` (+1).

Claude (2026-10-07, card coverage round 45 — COMPLETE): 16,794 → 16,841 / 34,913
(48.2%); all 50 faces mine; zero lost against HEAD.
- "Create X <tokens>" when X is defined (an {X} cost or a "where X is" clause), via the
  "create two …" reading with `count: Value::X` — Krenko Mob Boss, Dockside Extortionist,
  Decree of Justice, Elenda, Goblin Offensive, Gelatinous Genesis.
- Found by probing each blamed trigger's effect under "When ~ enters, <effect>."
- Tests: `create_number.rs` (+1).
Handoff: `docs/sessions/2026-10-07-create-x-tokens.md`.

Claude (2026-10-07, card coverage round 44 — COMPLETE): 16,781 → 16,794 / 34,913
(48.1%); all 13 faces mine; zero lost against HEAD.
- Trigger heads: "a source deals damage to ~" (`TakesDamage`), "a <noun> deals combat
  damage to you", "each player's first main phase", "combat on each opponent's turn",
  "~ is put into your graveyard from the battlefield" (dies, and you own it), "you cast
  your first spell each turn", "enchanted player is attacked" (`AttacksPlayer`),
  "enchanted creature becomes the target of a spell or ability"; self triggers now read
  "… for the first time each turn" (`turn_qualifier`).
- Tests: `new_triggers.rs` (+1).
Handoff: `docs/sessions/2026-10-07-trigger-heads.md` (updated).

Claude (2026-10-07, card coverage round 43 — COMPLETE): 16,760 → 16,781 / 34,913
(48.1%); all 22 faces mine; zero lost against HEAD.
- More trigger heads (self table and the noun-subject table): "deals combat damage to a
  player or planeswalker / or battle" (`AnyOf` of the two `DealsDamage`), "deals [combat]
  damage to a creature", "you scry or surveil" (`AnyOf`), "you discard one or more cards"
  (once per batch).
- Tests: `new_triggers.rs` (+2).
Handoff: `docs/sessions/2026-10-07-trigger-heads.md` (updated).

Claude (2026-10-07, card coverage round 42 — COMPLETE): 16,711 → 16,760 / 34,913
(48.0%); all 49 faces mine; zero lost against HEAD.
- Found by probing every failing trigger head as "<head>, draw a card.":
  "whenever one or more <kind> counters are put on ~ / a <noun>" (`CounterPlaced`),
  "whenever one or more [<type>] cards leave your graveyard" (`ZoneChange` from the
  graveyard, once per batch), "whenever a <noun> attacks you [or a planeswalker you
  control]" (new `EventPattern::AttacksPlayer`).
- **Engine fix (detect.rs):** a `ZoneChange` pattern's filter was checked only against
  the card's old identity, which no longer exists once a card leaves a non-battlefield
  zone (no last-known information there) — such triggers never fired. Now it falls back to
  the new identity outside the battlefield.
- Tests: `new_triggers.rs` (3, new).
Handoff: `docs/sessions/2026-10-07-trigger-heads.md`.

Claude (2026-10-07, card coverage round 41 — COMPLETE): 16,669 → 16,711 / 34,913
(47.9%); all 44 faces mine; zero lost against HEAD.
- More `value_phrase`: "your starting life total" (`Value::StartingLife`,
  `PlayerState::starting_life`), "the amount of life you gained this turn"
  (`Value::LifeGainedThisTurn`, `state.life_gained_amount`), "twice <v>", "half <v>,
  rounded up/down", "<n> plus <v>", "the number of colors among …" (`Value::ColorsAmong`),
  "the number of cards you've drawn this turn", "their/his/her <stat>" as "its".
- Nouns: "<cards> in all graveyards" (the Lhurgoyf cycle, Cruel Somnophage, Necrogoyf).
- Tests: `value_phrases.rs` (+3).
Handoff: `docs/sessions/2026-10-07-more-value-phrases.md`.

Claude (2026-10-07, card coverage round 40 — COMPLETE): 16,588 → 16,669 / 34,913
(47.7%); 80 faces mine, 1 Codex's; zero lost against HEAD.
- `value_phrase`: "your life total", "your devotion to <color>[ and <color>]" (new
  `Value::Devotion`, CR 700.5 — hybrid/Phyrexian symbols count), "the number of opponents
  you have", "… basic land types among lands you control", "… card types among cards in
  all graveyards", "the number of <kind> counters on ~/it", "that spell's …" (never the
  source itself — refused when "it" is still the source, e.g. Draining Whelk).
  `counted`: "… in all graveyards".
- Tests: `value_phrases.rs` (4, new): devotion with hybrid and its own symbols, a
  sacrificed source's counters, the triggering spell's mana value, opponents.
Handoff: `docs/sessions/2026-10-07-value-phrases-devotion.md`.

Claude (2026-10-07, card coverage round 39 — COMPLETE): 16,571 → 16,588 / 34,913
(47.5%); 13 faces mine, 4 Codex's; zero lost against HEAD.
- `Restriction::PlayLandsFromGraveyard` — "You may play lands from your graveyard"
  (Crucible of Worlds); `legal_actions` offers those lands as the turn's land play.
- `Effect::AddManaAnyCombination` — "add N mana in any combination of colors" on spells
  and non-mana triggers, a color asked per mana; refused inside activated abilities
  (mana abilities can't ask). Manamorphose, Cosmic Crucible.
- `Effect::RevealRandom` — "<player> reveals a card at random from their hand", binding
  it as "that card" / "the revealed card" (always the binding, even in a "where X is"
  read before the reveal). Ignite Memories, the Planeswalker's cycle, Merfolk Spy.
- Tests: `graveyard_lands.rs` (2), `reveal_random.rs` (2), both new;
  `announced_mana.rs` (+1).
Handoff: `docs/sessions/2026-10-07-crucible-mana-combos-random-reveal.md`.

- Completed launch-time automatic package updates for Windows/macOS/Flatpak. Checks latest stable GitHub release, verifies package size and SHA-256, gates startup before opening data, and restarts with path overrides retained. Existing SQLite/WAL/decks remain untouched; native swaps roll back on failure. Release workflow now publishes signed macOS ZIP bundles and tests native installer scripts. Validation: 1,745 workspace tests passed, 44 ignored; formatting, workspace Clippy, and release/installer Python checks passed. Native package execution awaits native CI.

Claude (2026-10-07, card coverage round 38 — COMPLETE): 16,562 → 16,571 / 34,913
(47.5%); 8 faces mine, 1 Codex's; zero lost against HEAD.
- Enduring: "When ~ dies, if it was a creature, return it … It's an enchantment." — the
  redundant "if it was a creature" is dropped on a self-dies trigger; "It's an
  enchantment." after a return to the battlefield is `SetTypes([Enchantment])` on the
  returned object (`Binding::It`), permanently.
- `Effect::Choose { except }` — "Choose a creature type other than Wall".
- Tests: `enduring.rs` (1, new), `choose_on_resolution.rs` (+1).
Handoff: `docs/sessions/2026-10-07-enduring-and-excluded-types.md`.

Claude (2026-10-07, card coverage round 37 — COMPLETE): 16,546 → 16,562 / 34,913
(47.4%); 15 faces mine, 2 Codex's; zero lost against HEAD.
- `Modification::SetCreatureTypes` (layer 4: replaces creature types, clears changeling)
  and `Modification::BecomesChosen(EntryChoice)`, turned into `SetCreatureTypes` /
  `SetColors` from the source's choice as the effect begins.
- `clauses::becomes_chosen`: "~ becomes the creature type / color of your choice until end
  of turn" (with `Effect::Choose`), "… becomes the chosen type/color …".
- Note for Codex: `mtg-app` lib tests didn't compile for me at 12:05
  (`update/mod.rs:493`, `{digest:x}` on a GenericArray) — looks mid-edit, left alone.
- Tests: `choose_on_resolution.rs` (+2).
Handoff: `docs/sessions/2026-10-07-becomes-chosen.md`.

Claude (2026-10-07, card coverage round 36 — COMPLETE): 16,517 → 16,546 / 34,913
(47.4%); of the faces gained since my round-35 list (taken at 16,508), 14 mine
and 27 Codex's; zero lost against HEAD.
- "You may pay {X}. If you do / When you do, … X …": `MayPay` asks X (up to what can be
  paid) after the yes, and fixes it into the effect (`walk::substitute_value`), so a
  reflexive trigger keeps it. Compiler sets `cx.x` for that body.
- "… you may …. Do this only once each turn.": `Effect::OnceEachTurn` +
  `Effect::MarkOnceEachTurn` in the yes branch; `state.done_once_this_turn`, cleared each
  turn — declining doesn't use it up.
- **For Codex (UI):** a "choose a value for X" question during a trigger's resolution.
- Tests: `pay_x.rs` (3), `once_each_turn.rs` (1), both new.
Handoff: `docs/sessions/2026-10-07-pay-x-and-once-each-turn.md`.

- Completed compound untap-and-goad and leading end-of-turn control changes. Gameplay verifies a shared target, haste/control expiring at cleanup, and goad lasting through the opponent’s turn. Live coverage: **16,546 / 34,913 (47.4%)**.

- Completed until-your-next-turn boosts and keyword grants; fixed existing duration never expiring. Effects end at the start of their controller’s next turn, before untap decisions. Gameplay checks opposing ownership, persistence through opponent turn, and preservation of printed keywords. Live coverage: **16,539 / 34,913 (47.4%)**.

- Completed removal of all counters of a specified type, with per-object counts at resolution. Gameplay checks targeted and all-creature effects, exact removal event amounts, preservation of other counter types, and source-only end-step triggers. Live coverage: **16,532 / 34,913 (47.4%)**.

- Completed opponent-graveyard card nouns (from/in): restrict targets by graveyard zone and opponent ownership, including typed creature cards. Gameplay covers exclusion of own cards/other zones and reanimation preserving ownership. Live coverage: **16,517 / 34,913 (47.3%)**.

Claude (2026-10-07, card coverage round 35 — COMPLETE): 16,475 → 16,517 / 34,913
(47.3%); 28 faces mine, the rest Codex's; zero lost against HEAD.
- `its_controller_may` now takes any non-"you" player subject from `nouns::player`
  ("each player may", "each opponent may", "target opponent may", "that player may",
  "defending player may", "that creature's controller may"); refused when the clause
  itself says "you". Show and Tell, Veteran Explorer, Fecundity, Jungle Wayfinder.
- `ActivationTiming::AnyPlayer` / `AnyPlayerSorcery` — "Any player may activate this
  ability" (CR 602.1b); offered to every player with priority.
- **Engine fix:** an activated ability on the stack was controlled by its source's
  controller; it is now controlled by the player who activated it (CR 602.2a).
- **For Codex (UI/bot):** opponents' permanents can now offer `ActivateAbility` to a player.
- Tests: `its_controller_may.rs` (+2), `any_player_activates.rs` (2, new).
Handoff: `docs/sessions/2026-10-07-player-may-and-any-player.md`.

- Completed standalone hand reveals: target player/opponent, you, and each player; compound life-loss shares its player target. Gameplay regressions cover empty hands, ownership, and cards remaining in hand. Live coverage: **16,508 / 34,913 (47.3%)**; includes concurrent work.

Codex (2026-10-07, end-of-combat durations — COMPLETE): suffix parsing for
boosts/keywords, control, and animation; combat-only activation restrictions.
Combat clearing and expiry now occur when EndCombat ends rather than begins.
Four gameplay regressions cover duration boundaries and activation legality.
Engine/Oracle: 1,241 passed, 38 ignored; workspace all-feature Clippy and formatting
pass. Audit: 16,478 / 34,913 playable (47.2%), +3 over prior snapshot.
Handoff: `docs/sessions/2026-10-07-combat-durations.md`.

Claude (2026-10-07, card coverage round 34 — COMPLETE): 16,454 → 16,475 / 34,913
(47.2%); 14 faces mine, 9 Codex's; zero lost against HEAD.
- `Effect::AsPlayer { who, body }` — another player acts as "you" within `body` (their
  "may", choices, library, shuffle); targets and bindings shared. Compiled for "its
  controller may …" (`clauses::its_controller_may`, "their library/hand" read as
  "your …"): Path to Exile, Assassin's Trophy, Ghost Quarter, Erode, Synapse Sliver.
  **For Codex (UI/bot):** the opponent now gets a "may" yes/no and a search choice in the
  middle of the caster's spell.
- Tests: `its_controller_may.rs` (2, new).
Handoff: `docs/sessions/2026-10-07-its-controller-may.md`.

Codex (2026-10-07, self attacks-alone — COMPLETE): added AttackingAlone filter
in eval/detect/walk; compiles source attacks-alone triggers. Generic attacks-alone
and exalted now test the attack event rather than an intervening-if on resolution.
Four regressions include adding an attacker after triggers reach the stack.
Engine/Oracle/IR: 1237 passed, 38 ignored; workspace all-feature Clippy
and formatting pass. Audit: 16,475 / 34,913 playable (47.2%).
Handoff: `docs/sessions/2026-10-07-self-attacks-alone.md`.

Claude (2026-10-07, card coverage round 33 — COMPLETE): 16,420 → 16,454 / 34,913
(47.1%); 15 faces mine, 19 Codex's; zero lost against HEAD.
- Dice (CR 706): `Effect::RollDie { sides, outcomes, then }`, `Value::RollResult` ("the
  result"), `Event::DieRolled`. Results-table rows ("1—9 | …") are compiled by a pre-pass
  in `compile()` and attached to the line's roll; rows that target are refused.
- `walk::Visitor` gained a `value` callback; `walk::substitute_value` replaces "the result"
  with the number rolled before the row resolves (X stays the spell's X).
- "Create a number of … tokens equal to …" (`create_token`).
- Tests: `dice.rs` (3), `create_number.rs` (2), both new.
Handoff: `docs/sessions/2026-10-07-dice-and-token-counts.md`.

Codex (2026-10-07, captured delayed targets — COMPLETE): simple delayed
Destroy/MoveZone/Tap/Untap capture original targets through Let and a saved binding;
future actions do not target or affect a returned permanent's new identity. Three
regressions pass. Engine/Oracle: 1229 passed, 38 ignored; workspace
all-feature Clippy and formatting pass. Audit: 16,454 / 34,913 playable (47.1%).
Handoff: `docs/sessions/2026-10-07-captured-delayed-targets.md`.

Codex (2026-10-07, targeted hand-to-library placement — COMPLETE): targeted
players/opponents choose fixed-count cards from their own hands for top/bottom
placement, with owner ordering. Two regressions cover privacy, order, and empty
hands. Engine/Oracle: 1223 passed, 38 ignored; workspace all-feature
Clippy and formatting pass. Audit: 16,432 / 34,913 playable (47.1%), +4.
Handoff: `docs/sessions/2026-10-07-targeted-hand-library.md`.

Codex (2026-10-07, reveal casting costs — COMPLETE): accepts fixed-count
reveal-from-hand additional costs and reveal-or-mana choices. Cards are revealed
with CostPayment events during casting and remain in hand. Four regressions cover
payment alternatives, affordability, private hand boundaries, timing, and printed
Thunderherd Migration / Goldmeadow Stalwart. Engine/Oracle: 1,221 passed, 38 ignored;
workspace all-feature Clippy and formatting pass. Audit: 16,428 / 34,913 (47.1%).
Handoff: `docs/sessions/2026-10-07-reveal-casting-costs.md`.

Claude (2026-10-07, card coverage round 32 — COMPLETE): 16,388 → 16,420 / 34,913
(47.0%); 17 faces mine, 16 Codex's; zero lost against HEAD.
- "If ~ would be put into a graveyard from anywhere, reveal ~ and shuffle it into its
  owner's library instead": `state.shuffled_instead_of_graveyard` (like the exile one);
  `apply` follows the redirected move with `Shuffled`.
- `Restriction::Protection { spares_source }` — "This effect doesn't remove this Aura".
- `Effect::ExileSelfWithCounters` — "Exile ~ with three time counters on it" (suspend).
- Tests: `shuffle_instead.rs` (2), `aura_protection.rs` (2), `rift.rs` (1), all new.
Handoff: `docs/sessions/2026-10-07-shuffle-aura-protection-rift.md`.

Codex (2026-10-07, hand-to-library placement — COMPLETE): chooses fixed/any
number of hand cards for top/bottom placement using existing selection and ordering.
Supports draw-that-many-plus-N and optional placement followed by conditional draw;
empty hands cannot enable the conditional draw. Four gameplay regressions pass.
Engine/Oracle: 1217 passed, 38 ignored; workspace all-feature Clippy and
formatting pass. Live audit: 16,420 / 34,913 playable (47.0%).
Handoff: `docs/sessions/2026-10-07-hand-library-placement.md`.

Codex (2026-10-07, targeted-player counts — COMPLETE): resolving “for each”
counts now allocate a player/opponent target scoped to the controlled-object
filter; supports draws, life gain, and creature bonuses. Three regressions cover
current-state counting, separate target slots, and illegal player targets.
Engine/Oracle: 1,208 passed, 38 ignored; workspace all-feature Clippy and formatting
pass. Live audit: 16,393 / 34,913 playable (47.0%), +5 over starting snapshot.
Handoff: `docs/sessions/2026-10-07-targeted-player-counts.md`.

Claude (2026-10-07, card coverage round 31 — COMPLETE): coin flips and clash.
16,341 → 16,388 / 34,913 (46.9%); 37 faces from this round, 10 Codex's; zero lost
against HEAD. `Effect::FlipCoin` (the state's seeded RNG, so a replayed resolution flips
the same way) and `Effect::Clash` (each reveals, keeps on top or puts on the bottom; win
on higher mana value); `Event::CoinFlipped` / `Event::Clashed` in the log. **For Codex
(UI):** a "keep the revealed card on top of your library?" yes/no for each clashing player
(with 3+ players, a "clash with which opponent" choice). Tests: `flips_and_clashes.rs`.
Handoff: `docs/sessions/2026-10-07-flips-and-clashes.md`.

Codex (2026-10-07, return-land alternative costs — COMPLETE): added
AdditionalCost::ReturnToHand, chosen through the existing payment path. Compiler
accepts return-one/two/three Island and pay-mana + return-basic-land alternatives.
Returns happen during payment, to the owner; a returned land can pay the mana.
Four regressions pass, including countering after payment and normal casting.
Engine/IR/Oracle: 1202 passed, 38 ignored; workspace all-feature Clippy
passes. Live audit: 16,351 / 34,913 playable (46.8%), +10 over prior snapshot.
Handoff: `docs/sessions/2026-10-07-return-land-alternative-costs.md`.

Codex (2026-10-07, untap resolution choices — COMPLETE): compiles non-targeted
“untap up to N lands” through ChosenBy; Untap now asks during resolution. Four
regressions cover selection counts, control restrictions, invalid picks, printed
texts, and entry triggers. Engine/Oracle: 1,194 passed, 38 ignored; formatting
and all-target Clippy pass. Live audit: 16,341 / 34,913 playable (46.8%).
Handoff: `docs/sessions/2026-10-07-untap-resolution-choices.md`.

Codex (2026-10-07, pipeline formatting and source-owner shuffle): applied
`cargo fmt --all`; the exact CI formatting check and workspace all-feature
Clippy pass. Release automation: six tests pass. Added source-owner shuffle
wording with a regression for differing owner/controller (Cerulean Sphinx).
Live audit: 16,311 / 34,913 playable as printed (46.7%). Full workspace all-feature tests pass outside the sandbox; sandbox socket
restrictions caused the initial local networking failures.

Codex (2026-10-07, life buyback/kicker — COMPLETE): accepts fixed PayLife costs;
paid kicker parts join spell_extra_cost, reserving/paying life through the existing
path. Affordability reserves life for combined costs and Phyrexian mana. Six
regressions pass, including full Slaughter text and life-zero boundary.
Full engine/Oracle: 1,187 passed, 38 ignored; all-target Clippy passes.
Handoff: `docs/sessions/2026-10-07-life-buyback-kicker.md`.

Codex (2026-10-07, owner-targeted shuffle — COMPLETE): accepts Deglamer/Unravel's
choose-target + owner shuffle and explicit owner-of-target wording; complete
Cathartic Parting follow-up also tested. Owner/controller differences and illegal
targets covered. Full engine/Oracle: 1,181 passed, 38 ignored; Clippy passes.
No engine/IR edits; exert regressions now pass.
Handoff: `docs/sessions/2026-10-07-owner-targeted-shuffles.md`.

Codex (2026-10-07, mana announcement views — COMPLETE): player projection hides
temporary announced mana objects from both visible objects and the public stack.
Parameterized real-engine regression covers tap-creature/counter-X costs, both
players, empty stack and a real spell on stack. Source and real spell remain visible;
mana still arrives immediately. All 230 app library tests pass; engine/Oracle
all-target Clippy passes. Broad Oracle run has a concurrent exert untap regression
failure; this view-only change does not alter gameplay state.
Handoff: `docs/sessions/2026-10-07-mana-announcement-views.md`.

Codex (2026-10-07, selected graveyard shuffles — COMPLETE): compiler accepts
targeted own-graveyard cards shuffled into your library; intermediate Natural
placement avoids ordering immediately before a shuffle, including whole-zone
shuffles. Four regressions pass. Full engine/Oracle: 1,170 passed, 38 ignored;
all-target Clippy passes. No engine/IR edits; concurrent mana work preserved.
Handoff: `docs/sessions/2026-10-07-selected-graveyard-shuffles.md`.

Codex (2026-10-07, plural library placement — COMPLETE): compiler accepts plural
targeted placement into owners' libraries, including any-number own-graveyard
targets. MoveZone asks each owner for top/bottom ordering. Five regressions pass;
full engine/Oracle run passed 1,163 tests, 38 ignored before final two regressions.
All-target Clippy passes. Stayed outside Claude's active mana work.
Handoff: `docs/sessions/2026-10-07-plural-library-placement.md`.

Codex (2026-10-07, combined Aura restrictions — COMPLETE): static restriction
parser emits untap and activated-ability suppression for the combined sentence.
Full printed Encrust, Stuck in Summoner's Sanctum, Hold for Questioning text compiles.
Gameplay verifies suppression, unaffected control, and restoration after Aura
removal. Full Oracle run: 889 passed, 15 ignored; printed-text regression passes
separately. Depletion land now passes. No engine/IR changes.
Handoff: `docs/sessions/2026-10-07-combined-aura-restrictions.md`.

Codex (2026-10-07, nonmana buyback — COMPLETE): compiler accepts Buyback— chosen
sacrifice/discard/tap costs on instants and sorceries using existing engine payment.
Paid/declined sacrifice and discard gameplay regressions pass, as does mana buyback.
Full Oracle run: 83 unit tests and 756 compiled tests passed; 15 ignored; depletion
land activation regression fails NotLegal (no buyback involved). Concurrent work
preserved. Handoff: `docs/sessions/2026-10-07-nonmana-buyback.md`.

Codex (2026-10-07, Leyline client follow-up — COMPLETE): bot now selects offered
opening battlefield cards rather than applying the discard heuristic. Existing
UI supports none/some/all, verified with pointer-click regressions. All 41 policy
tests and focused UI test pass. Handoff:
`docs/sessions/2026-10-07-leyline-client-choices.md`.

Codex completed deck-builder undo/redo in `crates/mtg-app/src/builder.rs`
and its tests. All 93 client library tests pass, and client Clippy is clean.
Handoff: `docs/sessions/2026-09-30-deck-builder-history.md`.
Codex left rules-engine, compiler, core, format, and shared README edits untouched.

Codex also completed card-support labels and explanations in the deck builder,
confined to `crates/mtg-app/src/builder.rs` and its own tests/handoff.
All 96 client library tests pass; client Clippy is clean.
Handoff: `docs/sessions/2026-09-30-deck-builder-support.md`.

Codex completed London mulligan practice in the sample-hand window in
`crates/mtg-app/src/builder.rs`. All 101 client library tests pass; client Clippy
is clean. Handoff: `docs/sessions/2026-09-30-sample-mulligans.md`.

Codex completed deck-builder validation using the active store, explicit missing
card messages, and an expandable list of all problems. All 105 client library tests
pass; client Clippy is clean. The temporary `Subtypes::intern` signature error
Claude noted is fixed (`Option<u16>`). Code edits remain in builder.rs.
Handoff: `docs/sessions/2026-09-30-builder-validation.md`.

The user reports Claude is working on changeling and damage prevention.

Claude is working on changeling, damage prevention, granted non-keyword abilities and
copy effects: `mtg-core`, `mtg-ir`, `mtg-engine`, `mtg-oracle` (compiler + tests). It will
stay out of `mtg-app/src/builder.rs`.

- Done (uncommitted): changeling; prevention shields + static prevention
  (`mtg-engine/src/prevention.rs`); granted activated/mana/triggered abilities
  (`mtg-engine/src/abilities.rs`, ids from `AbilityId::GRANTED_BASE`); copy effects —
  clones ("enter as a copy") and token copies (`GameObject::original`, `Event::BecameCopy`).
- Outside Claude's crates: one line in `mtg-app/src/format.rs` (new `Characteristics`
  fields), two in `mtg-policy/src/bot.rs`, and `mtg-store` now interns every creature
  type on open (token-only types like Scion).
- Done (uncommitted, Claude): affinity and self cost reductions, echo, buyback, unearth
  and graveyard-activated abilities, storm and spell copies, delve, soulshift, bounce
  lands, "becomes the target … sacrifice it", explore, control Auras, shock lands,
  modular, fading, cumulative upkeep, evolve, bloodthirst, fabricate, living weapon.
  Then: casting during resolution (cascade, rebound, madness), morph/megamorph (new
  `Action::CastFaceDown`; one line each in `mtg-app` format.rs and board.rs), "exile …
  until ~ leaves the battlefield", multi-target nouns, and more grammar.

Codex added the Constructed four-copy check across the main deck and sideboard in
`mtg-app/src/builder.rs`. Handoff: `docs/sessions/2026-09-30-constructed-sideboard-copy-limit.md`.
`cargo check`, client Clippy, and the GUI build passed. No tests were run.

Codex also improved deck-builder empty search feedback and pagination: the stale
legality-data warning now checks database-wide metadata, and Next is enabled only
when an additional result exists. Handoff: `docs/sessions/2026-09-30-builder-search-feedback.md`.

Codex moved the Constructed four-copy limit into the shared `deck_problems`
validator, counting main deck and sideboard. Builder and game-start validation
now agree. Handoff: `docs/sessions/2026-09-30-shared-copy-limit.md`.

Codex added shared validation rejecting sideboard cards in Commander decks,
because Commander setup only uses the 99-card main deck and has no sideboarding.
Handoff: `docs/sessions/2026-09-30-commander-sideboard-validation.md`.

Codex updated constructed and Commander copy checks to count English card names,
including Commander duplicates matching the commander's name. Handoff:
`docs/sessions/2026-09-30-english-name-copy-limits.md`.

Codex bounded accepted peer WebSocket upgrades to ten seconds, capped WebSocket
messages at 64 KiB, and capped reassembled Noise frames at 32 MiB. Handoff:
`docs/sessions/2026-09-30-websocket-handshake-timeout.md`.

Codex tightened Noise header/ciphertext validation and added four regression
checks for malformed and oversized encrypted frames. All 80 transport-enabled
network tests pass, including real loopback sockets; transport-enabled Clippy and
workspace compilation pass. Handoff:
`docs/sessions/2026-09-30-noise-frame-validation.md`.

Codex added ten-second read/write timeouts for encrypted identity and lobby setup
in `mtg-app/src/net.rs`, clearing them before match play. Two new loopback
regression tests pass (seven socket tests total). Workspace compilation initially
passed; subsequent app Clippy encountered concurrent engine work referencing
`mtg_ir::walk::time_counter`. Handoff:
`docs/sessions/2026-09-30-encrypted-setup-timeouts.md`.
- Note for Codex (Claude, 2026-09-30): `builder::tests::validation_reports_structure_errors_from_the_active_cards`
  currently fails ("5 copies of Stone Bear" not reported) — looks tied to the in-progress
  sideboard copy-limit change in `mtg-session/src/game.rs`; Claude hasn't touched either file.

Codex bounded outbound TCP and WebSocket setup, retained buffered messages after
upgrade, and normalized IPv6 endpoint formatting in `mtg-net/src/ws.rs`. All 84
transport-enabled network tests pass; network Clippy and workspace compilation
pass. DNS still uses the blocking system resolver. Handoff:
`docs/sessions/2026-09-30-outbound-websocket-setup.md`.

Codex keeps the host lobby open after encrypted setup failures and rejected
invites, while preserving single-use behavior after an accepted Join. Four
loopback regression tests, app compilation, and app Clippy pass. Handoff:
`docs/sessions/2026-09-30-host-lobby-recovery.md`.
Full app library validation now passes all 109 tests. Also corrected the stale
builder test expectation to count main deck plus sideboard (21 Stone Bears).

Codex extended guest address fallback through encrypted host authentication in
`mtg-app/src/net.rs`. An endpoint that upgrades WebSocket but fails Noise identity
setup no longer prevents trying the next address. All 112 app tests pass. Normal
Clippy encountered a concurrent engine argument-count warning. Handoff:
`docs/sessions/2026-09-30-authenticated-endpoint-fallback.md`.
Follow-up Clippy was blocked by in-progress renown variants in core/IR; the
passing app suite preceded those concurrent edits. No engine changes were made.

Codex wired the UI cancellation flag into guest connection setup through
`join_with_cancel`, preserving the original `join` API. Two new cancellation
regressions pass; all 114 app library tests pass. App Clippy with dependency lints
disabled passes. Handoff: `docs/sessions/2026-09-30-join-cancellation.md`.
Both full network-match integration tests also pass, including play to completion.
- Done (uncommitted, Claude): suspend, dash, evoke (`Action::CastAlternative`; one more
  line each in `mtg-app` format.rs and board.rs), split second, flanking, unleash, riot,
  renown, extort, afterlife. Fixed: resolved abilities used to linger in exile as copies
  of their source's card; they now cease to exist.
- Note for Codex: `crates/mtg-app/tests/network_match.rs` doesn't compile against the
  new `net::join` signature (6 args) — in your area; Claude hasn't touched mtg-net.

Codex bound the host lobby's claimed guest identity to the Noise-authenticated
peer before accepting Join. Forged identities are refused without consuming the
invite. All 115 app library tests pass, including the new real-loopback regression.
Handoff: `docs/sessions/2026-09-30-authenticated-lobby-identity.md`.
The earlier note about the six-argument `join` API is resolved: `join` retains its
original signature, and the UI uses the new `join_with_cancel` entrypoint.
Final checks also pass: 84 network tests, both network-match integration tests,
and app Clippy with dependency lints disabled.

Codex enforced guest lobby ordering: Accepted must name the invited host and
precede Reveal; duplicate acceptance is rejected. Three regressions pass along
with all 87 network tests. Network Clippy and diff checks pass. Handoff:
`docs/sessions/2026-09-30-guest-lobby-ordering.md`.
App verification passes all 115 library tests and both network-match tests.

Codex extended resolved-address fallback through WebSocket upgrade failures in
`mtg-net/src/ws.rs`. Two new deterministic regressions pass; all 89 network tests,
network Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-09-30-resolved-address-fallback.md`.

Codex capped displayed lobby status history at 100 messages, made it scrollable,
and bounded UI event draining to 128 events per frame with immediate continuation
repaints. Code edits are in `mtg-app/src/app.rs`. Handoff:
`docs/sessions/2026-09-30-lobby-status-history.md`.
GUI build, app Clippy with dependency lints disabled, and diff checks pass.

Codex added independent target-slot selection and confirmation for multi-slot
questions, preserving single-slot immediate answers. Handoff:
`docs/sessions/2026-10-01-multiple-target-slots.md`.
Both target-selection regressions, GUI build, and app Clippy pass.

Codex added cleanup discard controls using exact-count selections from the
viewer's visible hand, and made optional single-mode choices skippable. Handoff:
`docs/sessions/2026-10-01-cleanup-discard-controls.md`.
All three UI selection tests and app Clippy pass.

Codex added named blocker-order controls and confirmation instead of default-only
answers. Handoff: `docs/sessions/2026-10-01-blocker-order-controls.md`.
GUI build and app Clippy pass.

Codex bounded decoded card-image dimensions and decoder allocations, with
regressions for small compressed images exceeding either dimension. Handoff:
`docs/sessions/2026-10-01-card-image-decode-limits.md`.
All 11 art tests and app Clippy pass.

Codex bounded board observer processing to 128 views per frame, with immediate
repaint and older-observation draining before question delivery. Handoff:
`docs/sessions/2026-10-01-board-view-batching.md`.
All four UI tests, app Clippy, and diff checks pass.

Codex made builder deck replacement/rename atomic in SQLite so write failures
preserve the original saved deck. Handoff:
`docs/sessions/2026-10-01-atomic-deck-rename-save.md`.
All 29 builder tests, all 17 store tests, and app/store Clippy pass.

Codex made Draft::spec fallible so playtesting cannot silently cap counts or drop
missing cards. Builder validation and Playtest now surface conversion errors.
Handoff: `docs/sessions/2026-10-01-exact-draft-conversion.md`.
All 30 builder tests, app Clippy, and diff checks pass. Draft::spec now returns
Result; workspace callers are updated.

Codex also removed silent count capping from saved-deck match conversion.
Handoff: `docs/sessions/2026-10-01-exact-saved-deck-conversion.md`.
All four deck tests and app Clippy pass.

Codex rejected companion/multiple-commander imports before saving rather than
silently changing their command sections. Saved-deck conversion also validates
commander cardinality. Handoff:
`docs/sessions/2026-10-01-command-section-import-validation.md`.
All five deck tests pass.

Codex surfaced deck-delete failures and preserves a different selected deck on
successful deletion. Handoff:
`docs/sessions/2026-10-01-deck-delete-error-reporting.md`.

Codex gave card-art caches stable full-name digests and exclusive temporary files
for concurrent writes. Handoff:
`docs/sessions/2026-10-01-card-cache-identity-and-writes.md`.
All 13 art tests and app Clippy pass. Old ambiguous cache files are retained but
not reused; requested art is downloaded under new filenames.

Codex validates card/symbol caches before reuse, repairs corrupt entries through
the existing download path, and caches only successfully decoded responses.
Local cache reads are bounded. Handoff:
`docs/sessions/2026-10-01-cache-validation-and-repair.md`.
All 14 art tests, app Clippy, and diff checks pass.

Codex guarded builder loading against overflowing duplicate counts and multiple
commanders, and checked import summary totals before saving. Handoff:
`docs/sessions/2026-10-01-saved-deck-load-validation.md`.
All 31 builder tests, all five deck tests, and app Clippy pass.

Codex made game undo/pass shortcuts respect focused text/numeric inputs.
Handoff: `docs/sessions/2026-10-01-game-shortcut-focus.md`.

Codex completed the autonomous app/deck/card-art batch and integrated validation:
149 app library tests, both network-match integration tests, and all 17 store
tests pass (168 total). GUI build, app/store Clippy with dependency lints disabled,
and diff checks pass. Consolidated handoff:
`docs/sessions/2026-10-01-autonomous-app-progress.md`.
Changes remain uncommitted; concurrent engine/compiler edits were preserved.

Codex added headless pointer-click regressions for target, mode, X, and blocker
order controls. Handoff: `docs/sessions/2026-10-01-headless-question-controls.md`.
All ten UI tests, app Clippy, and diff checks pass.

Codex fixed sideboard transfer overflow at 255 copies and underflow on empty
source rows, preserving total card identity. Handoff:
`docs/sessions/2026-10-01-sideboard-count-boundaries.md`.
All three app event/transfer tests, app Clippy, and diff checks pass.

Codex completed the UI/sideboarding follow-up: all 156 app library tests and both
network-match integration tests pass. GUI build, app Clippy, and diff checks pass.
Integrated handoff: `docs/sessions/2026-10-01-ui-sideboarding-validation.md`.

Codex preserved pasted input after partial deck imports so unmatched/unreadable
lines remain available for correction. Fully matched imports still clear input.
Handoff: `docs/sessions/2026-10-01-retain-partial-import-input.md`.
All five deck tests, GUI build, app Clippy, and diff checks pass.

Codex clears stale import results when the pasted list or deck name changes.
Handoff: `docs/sessions/2026-10-01-clear-stale-import-results.md`.
GUI build, app Clippy, and diff checks pass.

Codex routes deck imports and their display refresh through the active Store
instead of opening another default-path connection. Handoff:
`docs/sessions/2026-10-01-import-active-store.md`.
All five deck tests, GUI build, app Clippy, and diff checks pass.

Codex preserves deck selections across same-name replacement IDs and refreshes
builder/deletion results through the active Store. Handoff:
`docs/sessions/2026-10-01-preserve-deck-selections.md`.
All four app event/selection/transfer tests, GUI build, app Clippy, and diff
checks pass.

Codex follows builder rename saves by remapping selected old IDs to the saved
new ID before refresh, preserving choices even when both name and ID change.
Handoff: `docs/sessions/2026-10-01-preserve-renamed-deck-choices.md`.
All five app event/selection/transfer tests, GUI build, app Clippy, and diff
checks pass.

Codex surfaced deck-screen database-open errors, replaced per-frame retry loops
with an explicit retry control, and disabled new-deck creation without a Store.
Handoff: `docs/sessions/2026-10-01-deck-database-open-errors.md`.
GUI build, app Clippy, and diff checks pass.

Codex added cancellation for armed deck-delete prompts and disarms them when
switching decks, editing, creating a deck, or leaving the deck screen. Handoff:
`docs/sessions/2026-10-01-cancel-deck-deletion.md`.
GUI build, app Clippy, and diff checks pass.

Codex validated accumulated deck-screen changes: all 158 app library tests and
both network-match tests pass. GUI build, app Clippy, and diff checks pass.
Handoff: `docs/sessions/2026-10-01-deck-screen-integration-validation.md`.

Codex made pending blockers undoable before older completed blocks or engine
actions, with a headless click regression. Handoff:
`docs/sessions/2026-10-01-pending-blocker-undo.md`.
All 11 UI tests, scoped app Clippy, and diff checks pass. Concurrent policy
dependency warnings were left to its owner.

Codex now reports a closed match-worker event channel without a final result as
an error, preserving queued final results and clearing stale sideboarding UI.
Two focused regressions, app Clippy with dependency lints disabled, and diff
checks pass. Handoff: `docs/sessions/2026-09-30-worker-shutdown-feedback.md`.
- Done (uncommitted, Claude): ninjutsu (new cost part + `Event::EnteredAttacking`),
  connive, "~ or another X enters", subtype permanent searches, kicked counters,
  "doesn't untap during its controller's next untap step", "can't attack unless
  defending player controls …", "can't be blocked by more than one creature".
- Claude touched `mtg-app/src/ui.rs` (mode picker accepts a range for "choose one or
  both") and `mtg-policy/src/lib.rs` (`ChoiceKind::ChooseModes` gained `min`).
- Note for Codex: players now have energy (`PlayerSummary::energy` in the view); the client
  doesn't display it yet. Also new: `ChoiceKind::ChooseModes` is now used for "choose a
  color / creature type" as permanents enter (labels are color/type names).
- Claude (2026-10-01): sagas, transform (incl. werewolves, day/night via `GameState::day`),
  rituals, energy, delirium, chosen color/type, optional untap. Not in the view yet: day/night.
- Claude (2026-10-01, cont.): levelers, Classes (`GameObject::class_level`,
  `ActivationTiming::ClassLevel`), backup, bestow (`GameObject::bestowed()`; a bestowed
  card is an Aura in `layers::compute`), "that much"/"that many" (`Value::EventAmount`,
  carried in a triggered ability's X slot), "one or more …" batch triggers
  (`TriggerLimit::OncePerBatch`). `PrintedCards::subtype_named` is new (default `None`).
  Not in the client yet: class level display; bestow shows as an ordinary
  `Action::CastAlternative`.
- Claude (2026-10-01, cont.): trigger conditions — "X or Y" self triggers (`AnyOf`),
  enchanted/equipped creature triggers (look-back via `GameObject::was_attached_to` and
  `detect::looking_back`, CR 603.10a), "you attack", turned face up, heroic, magecraft,
  nth spell / nth draw each turn, cast-this-spell triggers (function from the stack),
  "if you cast it" (`GameObject::cast_from`). Retrace / jump-start / escape
  (`CastFrom` costs may now carry additional costs; exile-from-graveyard costs are
  payable). `Effect::May` gained `otherwise` ("sacrifice it unless you …").
- Claude (2026-10-01, cont.): monarch (`GameState::monarch`, an emblem object "The
  Monarch" in the command zone carries its triggers), impulse draw
  (`GameObject::may_play`, `Effect::GrantPlay`; exile objects with permission are offered
  as Cast/PlayLand), stun counters (`CounterKind::Stun`, untap replaced in
  `apply::replace`), goad (`Restriction::Goaded`), amass, Blood tokens, Bestow-style
  morbid/"if you cast it" conditions. For Codex: the view doesn't show who is the monarch
  or which exiled cards are playable yet.
- Claude (2026-10-01, cont.): foretell/plot (`AbilityKind::ExileToCastLater`, special
  actions from hand; `GameObject::cast_later`), warp, disturb (`CastFrom::transformed`,
  cast as `CastFace{face: 1}` from the graveyard), overload (`AlternativeCost::instead`),
  disguise (face-down ward {2} via `AbilityId::DISGUISE_WARD`), dethrone, myriad (no-op in
  two-player), annihilator/afflict/ingest/battle cry, `Selector::DefendingPlayer`, Dig
  ("look at the top N…"), becomes-a-creature (manlands), turn-limited activations. UI
  notes for Codex: foretold cards are face-down in exile but castable by their owner; the
  special-action label for foretell/plot is generic.
- Claude: the view can now include a *library* object — the top card, when an effect lets
  its owner look at it any time or reveals it (`GameState::sees_top/top_revealed`). Such
  cards can also be cast from there ("cast creature spells from the top of your
  library"). Also new: reflexive triggers, extra turns, player hexproof, "can't gain
  life", Arrest-style "activated abilities can't be activated".
- Claude: Auras can be attached to players (`GameObject::attached_player`, curses); the view's ObjectView doesn't carry it yet. Also: mana spent per spell is recorded (`SpentToCast`), entwine, sunburst/converge, tokens entering attacking, mobilize.
- Claude: speed (`PlayerState::speed`, start your engines!/max speed) isn't in the view yet.
- Claude: restricted mana ("spend this mana only to cast …") is used only by automatic payment for a qualifying spell; such abilities aren't offered as ActivateManaAbility (the pool can't track restrictions). Speed/saddle/ward—discard/skip-draw also added.

Codex added a library-top browser for cards already disclosed in PlayerView,
addressing the library UI handoff. Zone play menus also preserve alternative
casting options. Code edits: mtg-app board.rs/ui.rs. Handoff:
`docs/sessions/2026-10-01-visible-library-browser.md`.
All 19 board tests, app compilation, app Clippy with dependency lints disabled,
and diff checks pass.

Codex named special actions (foretell, plot, suspend, turn face up) from compiled
ability data and displayed their immediate mana costs. Hidden/unknown identities
retain generic labels. Handoff:
`docs/sessions/2026-10-01-special-action-labels.md`.
All eight card-text tests, app compilation, app Clippy with dependency lints
disabled, and diff checks pass.

Codex carried public player energy counters through board arrangement and shows
nonzero totals in player headers. Extended the existing player-total regression.
Handoff: `docs/sessions/2026-10-01-energy-display.md`.
All 19 board tests, app Clippy with dependency lints disabled, and diff checks pass.

Codex replaced Rust Debug counter labels with readable names and singular/plural
totals across card notes, permanent summaries, and ordering explanations.
Handoff: `docs/sessions/2026-10-01-readable-counter-labels.md`.
All 12 formatting tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex included engine-offered special actions in hand-card menus, making foretell,
plot, and suspend available directly from their card. Multiple options require an
explicit choice. Handoff: `docs/sessions/2026-10-01-hand-special-actions.md`.
All 20 board tests, app Clippy with dependency lints disabled, and diff checks pass.

Codex added "Attached to …" state notes to Aura/Equipment cards using the existing
visible-object names. Player attachments still need attached_player in ObjectView.
Handoff: `docs/sessions/2026-10-01-attachment-state-notes.md`.
App Clippy with dependency lints disabled and diff checks pass.

Codex preserved separate priority cast choices for identical cards in different
zones and labeled non-hand play origins. Same-zone duplicates still collapse.
Handoff: `docs/sessions/2026-10-01-zone-specific-cast-choices.md`.
All 21 board tests, app Clippy with dependency lints disabled, and diff checks pass.

Codex labeled alternative casts by mechanic and mana cost (dash, evoke, overload,
warp, bestow), preserving hidden identity fallback. Handoff:
`docs/sessions/2026-10-01-alternative-cast-labels.md`.
All nine card-text tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex displayed announced targets on stack cards using existing ObjectView.targets
and visible names, for both image and text-card rendering. Handoff:
`docs/sessions/2026-10-01-stack-target-notes.md`.
App Clippy with dependency lints disabled and diff checks pass.

Codex validated the accumulated app/network work together: all 122 app library
tests and both network-match integration tests pass. GUI build, app Clippy with
dependency lints disabled, and diff checks pass. No code changes were needed.
Handoff: `docs/sessions/2026-10-01-app-integration-validation.md`.

Codex enforced Constructed deck/sideboard sizes after local and remote sideboarding
and disabled UI completion with an explanation for invalid sizes. Handoff:
`docs/sessions/2026-10-01-sideboarding-size-validation.md`.
All six match tests, app compilation, session/app Clippy with dependency lints
disabled, and diff checks pass.

Codex added public energy/poison gain and loss narration, including resulting
totals and safe unsigned deltas. Handoff:
`docs/sessions/2026-10-01-public-counter-narration.md`.
All nine narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex widened life narration arithmetic to i64 and added a regression for both
directions across the full i32 range. Handoff:
`docs/sessions/2026-10-01-life-narration-overflow.md`.
All ten narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex corrected death narration to match new graveyard arrivals rather than old
copies, and uses the departing card's visible face for creature classification.
Handoff: `docs/sessions/2026-10-01-graveyard-arrival-narration.md`.
All 11 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex added change-of-control narration for persistent battlefield objects,
without false battlefield-entry messages. Handoff:
`docs/sessions/2026-10-01-control-change-narration.md`.
All 12 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex replaced unsupported "resolved" claims with "left the stack" when a stack
object disappears without entering the battlefield. Handoff:
`docs/sessions/2026-10-01-stack-departure-narration.md`.
All 13 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex made hand-arrival narration factual ("Your hand gained …") rather than
assuming all additions were draws, and includes arrivals with unchanged hand size.
Handoff: `docs/sessions/2026-10-01-hand-arrival-narration.md`.
All 15 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex added readable counter gain/loss narration for persistent battlefield
objects, including resulting totals. Handoff:
`docs/sessions/2026-10-01-permanent-counter-narration.md`.
All 16 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex added attachment, movement, and detachment narration for persistent
battlefield objects, using only projected target names. Handoff:
`docs/sessions/2026-10-01-attachment-narration.md`.
All 17 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex changed land-arrival narration to "entered the battlefield" because views
cannot distinguish land plays from effect-driven arrivals. Handoff:
`docs/sessions/2026-10-01-land-arrival-narration.md`.
All 18 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex stopped matching hidden stack objects or abilities to unrelated permanent
arrivals when suppressing stack-departure messages. Handoff:
`docs/sessions/2026-10-01-stack-arrival-matching.md`.
All 20 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex fixed blocking narration to report changed attacker assignments while
keeping unchanged assignments quiet. Handoff:
`docs/sessions/2026-10-01-blocking-assignment-narration.md`.
All 21 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex added defending player names to attack narration and retained safe naming
for visible and unavailable permanent targets. Handoff:
`docs/sessions/2026-10-01-attack-target-narration.md`.
All 22 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex validated the accumulated app changes: all 136 library tests and both
network-match integration tests pass, including a hosted match played to
completion. GUI build, app Clippy with dependency lints disabled, and diff
checks pass. Handoff:
`docs/sessions/2026-10-01-app-followup-validation.md`.

Codex made the full legal X range selectable, with direct buttons for small
ranges and a bounded numeric editor for larger ranges. Handoff:
`docs/sessions/2026-10-01-full-x-choice-range.md`.
GUI build, app Clippy with dependency lints disabled, and diff checks pass.

Codex fixed target Undo to remove the most recently selected slot, including
out-of-order selections and explicit deselection. All 12 UI tests, app Clippy
with dependency lints disabled, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-target-selection-undo-order.md`.

Codex made Undo close pending card play-options menus before undoing game actions.
All 13 UI tests and app library/binary Clippy pass. All-target validation is
blocked by the separate unfinished untracked ui_rendering integration test.
Handoff: `docs/sessions/2026-10-01-play-options-undo.md`.

Codex added Cancel concede to the two-click concession controls. The regression
verifies cancellation sends no answer and subsequent confirmation still works.
All 14 UI tests, app library/binary Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-cancel-concession.md`.

Codex synchronized blocker panel assignments with pending battlefield selections,
preventing stale attacker clicks from reassigning completed blocks. All 15 UI
tests and app library/binary Clippy pass. Handoff:
`docs/sessions/2026-10-01-blocker-panel-selection.md`.

Codex dismissed stale unsaved-deck exit prompts after successful saves or edits
that restore the saved state. All 31 builder tests, app library/binary Clippy,
and diff checks pass. Handoff:
`docs/sessions/2026-10-01-dismiss-saved-deck-exit-prompt.md`.

Codex guarded additions and moves into full deck rows before mutations or undo
history changes. All 32 builder tests and app library/binary Clippy pass.
Handoff: `docs/sessions/2026-10-01-full-deck-row-edits.md`.

Codex widened builder deck totals, curve buckets, and card badges to u64 so
multiple valid u32 rows cannot overflow their aggregates. All 33 builder tests,
app library/binary Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-wide-deck-aggregates.md`.

Codex bounded sample-hand expansion to 10,000 main-deck cards and displays an
error for larger libraries before allocating. All 34 builder tests, app
library/binary Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-bound-sample-library.md`.

Codex validated the accumulated choice and builder fixes: all 166 app library
tests and both network-match integration tests pass, including a hosted match
played to completion. The GUI binary builds and diff checks pass. Handoff:
`docs/sessions/2026-10-01-choice-builder-integration-validation.md`.

Codex guarded one-copy removal from saved zero-count rows, preventing underflow
and preserving redo history. All 35 builder tests, app library/binary Clippy,
and diff checks pass. Handoff: `docs/sessions/2026-10-01-zero-row-removal.md`.

Codex surfaced builder card-search database errors with explicit retry and cleared
stale results instead of reporting no matches or missing legality. All 36 builder
tests, app library/binary Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-card-search-errors.md`.

Codex surfaced deck-refresh read errors and preserved the previous card count,
deck list, and choices on failure. Retry uses the active connection. All 6 deck
tests and app library/binary Clippy pass. Handoff:
`docs/sessions/2026-10-01-deck-refresh-read-errors.md`.

Codex bounded database progress draining to 128 updates per UI frame, retaining
completion behind backlogs and distinguishing worker shutdown. All 7 app event
tests, app library/binary Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-bounded-database-progress.md`.

Codex validated all app targets: 171 library + 2 network-match + 15 UI integration
tests pass (188 total). Package formatting, app all-target Clippy, and diff checks
pass. The repaired UI test received formatting and a Clippy initializer fix.
Handoff: `docs/sessions/2026-10-01-all-app-target-validation.md`.

Codex corrected deck coverage labels and the outdated vanilla/keyword-only hover
explanation to reflect distinct-card whole-rules/layout checks. Formatting,
all-target app Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-current-support-explanation.md`.

Codex cleared outdated builder status on name edits and successful sample-hand
generation. Formatting, all-target app Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-clear-stale-builder-status.md`.

Codex made default combat-damage object assignments visible before confirmation.
All 16 UI tests and all-target app Clippy pass. Follow-up for engine owner:
DamageAssignment rows currently lack legality validation, and canonical default
conversion drops player targets. Handoff:
`docs/sessions/2026-10-01-visible-default-combat-damage.md`.

Codex fixed accepting default multi-blocker trample damage: object-only wire rows
now recover the complete canonical assignment, preserving player damage. The
regression failed at 20 vs 18 life before the fix. All 25 combat tests and engine
library Clippy pass. Custom-row validation remains open. Handoff:
`docs/sessions/2026-10-01-default-trample-damage.md`.

Codex showed excess default trample damage and its projected defender in the
combat preview. All 17 UI tests, all-target app Clippy, formatting, and diff checks
pass. Handoff: `docs/sessions/2026-10-01-trample-damage-preview.md`.

Codex validated custom object-only combat damage against offered blockers,
distinct rows, full totals, and the engine's lethal-before-later-blocker ordering.
Rejected answers preserve their question. All 26 combat tests and scoped engine
Clippy pass. Player trample allocation in custom answers remains limited.
Handoff: `docs/sessions/2026-10-01-custom-combat-damage-validation.md`.

Codex validated combat changes across the full engine suite: enabled tests pass;
existing ignored ability-extension tests remain skipped. All 17 app UI tests,
all-target app Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-combat-engine-integration-validation.md`.

Codex fixed signed wrapping in lethal-damage calculations for large marked u32
damage. The regression reproduced subtraction overflow before the fix. All 27
combat tests and scoped engine Clippy pass. Handoff:
`docs/sessions/2026-10-01-lethal-damage-arithmetic.md`.

Codex verified non-default deathtouch damage allocation with a 1/2/1 split among
three 6-toughness blockers. All 28 combat tests and scoped engine Clippy pass.
Handoff: `docs/sessions/2026-10-01-deathtouch-custom-damage-regression.md`.

Codex widened combined blocker lethal thresholds for forced-damage detection.
The regression reproduced addition overflow before the fix. All 29 combat tests
and scoped engine Clippy pass. Handoff:
`docs/sessions/2026-10-01-combined-lethal-thresholds.md`.

Codex resolved custom trample excess: unused damage reaches the attack destination
only with trample and lethal assigned to every blocker. Wire shape unchanged.
All 30 combat tests and scoped engine Clippy pass. This supersedes the earlier
custom-player-trample limitation; a GUI allocation editor remains future work.
Handoff: `docs/sessions/2026-10-01-custom-trample-excess.md`.

Codex added custom blocker damage controls with default seeding, bounded amounts,
total confirmation checks, cancellation, and Undo. Engine validation re-asks
illegal allocations. All 18 UI tests and all-target app Clippy pass. Handoff:
`docs/sessions/2026-10-01-custom-combat-damage-controls.md`.

Codex added explicit retry feedback when an answered game question returns with
the same ID, excluding Undo to avoid false rejection messages. All 19 UI tests,
all-target app Clippy, and diff checks pass after concurrent API updates settled.
Handoff: `docs/sessions/2026-10-01-rejected-answer-feedback.md`.

Post-cleanup app validation passes: 192 app tests and 30 combat tests. The custom
damage editor now numbers blockers and shows unassigned or excess amounts.
All 19 UI tests and all-target app Clippy pass after the presentation change.
Handoff: `docs/sessions/2026-10-01-combat-editor-integration-check.md`.

Fixed accumulated marked-damage overflow and signed wrapping in the state-based
lethal check. Regressions reproduced both failures. All 32 combat tests, rules
integration tests, and scoped engine Clippy pass. Handoff:
`docs/sessions/2026-10-01-large-accumulated-damage.md`.

Card choices now render candidates and card hovers offer persistent enlargement.
Resolution questions preserve the chooser's pre-rollback view, fixing newly drawn
discard candidates while retaining opponent privacy. All 194 app tests, 23 session
tests, the headless bot test, the new draw/discard regression, and scoped Clippy
pass. Broad compiled suite has 10 trigger-related failures (419 pass, 15 ignored);
concurrent detection edits preserved. Handoff:
`docs/sessions/2026-10-01-card-choice-previews.md`.

Gameplay audit against the September 25 Comprehensive Rules fixes missed zone-change
observers, face-down creatures ignoring external tapped-entry effects, false tap
triggers on entry, obsolete combat assignment ordering, trample after blockers leave,
and first-strike history between damage steps. Basic instant/sorcery timing checks
pass. Workspace: 1,186 passed, zero failed, 38 ignored; final added combat and shock
land regressions also pass separately. Library/binary Clippy passes; all-target
Clippy reports existing test lints. Handoff:
`docs/sessions/2026-10-01-gameplay-rules-audit.md`.

Follow-up rules audit fixes cleanup priority when SBAs or triggers occur, repeating
cleanup after responses before ending the turn. Also adds cancellation of opposing
+1/+1 and -1/-1 counters as an SBA, preserving pre-SBA death information. Regressions
reproduced both issues. Final workspace: 1,193 passed, zero failed, 38 ignored;
scoped engine/rules and compiled oracle Clippy and diff checks pass. Handoff:
`docs/sessions/2026-10-01-cleanup-and-counter-audit.md`.

Codex completed an autonomous reliability audit on 2026-10-05: bot arithmetic and
double-strike heuristics, deck parsing and CLI imports, atomic card writes, checked
persisted deck/subtype/key values, consistent identity startup, and atomic bulk
imports that propagate database failures. The documented headless run command
works, and invalid CLI arguments report errors. Existing UI and concurrent
engine/IR/compiler implementation edits were preserved. Engine code edits are
limited to two test files; oracle changes are in the bulk importer and its new
atomicity suite. Final all-feature workspace tests: 1,243 passed, zero failed,
41 ignored. All-target/all-feature workspace Clippy with warnings denied passes.
Handoff: `docs/sessions/2026-10-05-autonomous-reliability-audit.md`.

Codex continued with import streaming and refresh metadata: timestamps now commit
with cards, metadata read failures propagate, gzip detection handles short reads,
gzip nesting is bounded, and concatenated gzip members are fully imported. A
truncated later member rolls back earlier rows. Seven new tests pass; final
all-feature workspace run: 1,250 passed, zero failed, 41 ignored. All-target,
all-feature Clippy with warnings denied and whitespace checks pass. Handoff:
`docs/sessions/2026-10-05-import-streaming-followup.md`.

Claude (2026-10-05, card coverage, uncommitted): filter lands (one mana ability per
bundle), "costs {N} less to cast if it targets <noun>" (`Condition::TargetsMatching`;
targets are now recorded before the cost is computed in `complete_announcement`; such
spells are offered when only the discount makes them affordable and the target prompt is
then narrowed to qualifying targets), and "can block an additional creature each combat" /
"any number of creatures" (`Restriction::BlockAdditional`, `combat::block_capacity`).
**Wire change:** `ChoiceKind::DeclareBlockers` gained `capacity: Vec<(ObjectId, u32)>`
(`#[serde(default)]`). A blocker blocking several attackers is asked to divide its damage
through `AssignCombatDamage` with `attacker` = the blocker and `among` = its attackers (any
division of all of it, CR 510.1d). Outside Claude's crates: `{ eligible, .. }` patterns in
`mtg-policy/src/bot/mod.rs` and `mtg-headless/src/trace.rs`; in `mtg-app/src/ui.rs`,
`block_capacity`/`add_block` (a picked blocker with spare capacity adds an attacker
instead of switching), plus one UI test. The bot still blocks one attacker per blocker.
Coverage 13,887 → 13,947. Workspace tests pass.

Codex (2026-10-05, card playability): typed discard cost grammar now accepts
creature/land/artifact/color/subtype/nonland/instant-or-sorcery card filters.
Shared parsing covers activated and mandatory additional casting costs, using
existing engine selection/payment. Three regressions pass, along with the
all-feature workspace suite and oracle all-target Clippy. Local database coverage
now 13,971 / 34,913 (40.0%; 13,947 before this batch). Concurrent compiler and
engine work preserved; changes remain uncommitted. Handoff:
`docs/sessions/2026-10-05-typed-discard-card-coverage.md`.

Codex (2026-10-05, card playability): mana abilities can now append fixed,
unconditional draws/life gain/life loss, retaining immediate mana-ability
resolution. Tests verify manual paid/sacrificing activation and automatic payment
both execute the draw. No engine implementation changes. Three regressions and
all 573 enabled oracle tests pass; oracle Clippy and whitespace checks pass.
Coverage 13,971 → 13,978 / 34,913. Handoff:
`docs/sessions/2026-10-05-mana-ability-followups.md`.

Codex (2026-10-05, card playability): flashback accepts supported nonmana costs
(life, discard, sacrifice, graveyard exile). Engine now pays flashback life and
reserves additional cost resources before validating one mana plan, preventing
unpaid resolution after a failed replan. Six regressions added. Workspace passed
1,272 tests, zero failures, 41 ignored; final additional double-sacrifice test also
passed. Engine/oracle Clippy clean. Coverage 13,978 → 13,983 / 34,913 (40.1%).
Handoff: `docs/sessions/2026-10-05-flashback-nonmana-costs.md`.

Codex (2026-10-05, card playability): protection grammar supports multicolored,
monocolored, colorless, and mixed “and from” qualities. Three regressions verify
keyword/granted parsing and actual targeting, blocking, and damage prevention.
No engine implementation changes. All 582 enabled oracle tests pass (15 ignored),
Clippy and whitespace checks pass. Coverage 13,983 → 13,992 / 34,913 (40.1%).
Handoff: `docs/sessions/2026-10-05-protection-qualities.md`.

Codex (2026-10-05, card playability): discard-hand/redraw grammar retains the
original hand count using an existing binding, including fixed additional draws.
Three regressions verify empty/full hands, opponent privacy, and prior hand-size
damage. No engine implementation changes. All 585 enabled oracle tests pass
(15 ignored), Clippy and whitespace checks pass. Coverage 13,992 → 13,998 /
34,913. Handoff: `docs/sessions/2026-10-05-discard-hand-redraw.md`.

Codex (2026-10-05, card playability): second/third-from-top library placement now
compiles for supported targets and graveyard self-placement abilities. Three
regressions verify exact ordering and full/empty/one-card libraries. No engine
implementation changes. All 588 enabled oracle tests pass (15 ignored); Clippy
and whitespace checks pass. Coverage 13,998 → 14,004 / 34,913 (40.1%). Handoff:
`docs/sessions/2026-10-05-indexed-library-placement.md`.

Codex (2026-10-05, card playability): empty-battlefield conditions support
Pestilence/Pyrohemia shapes. A response regression exposed missing intervening-if
checks on resolution; engine now rechecks before executing triggered effects.
Three regressions and the all-feature workspace suite pass; engine/oracle Clippy
and whitespace checks pass. Coverage 14,004 → 14,008 / 34,913 (40.1%). Handoff:
`docs/sessions/2026-10-05-empty-battlefield-triggers.md`.

Codex (2026-10-05, deck builder): user selected suggestions, synergy, and analysis.
Added theme searches beyond the first page, Commander identity filtering, weighted
role/mana analysis, CSV/JSON interchange, and bounded native file-drop imports.
Builder tests (42), deck tests (27), and app/deck Clippy pass. Existing app socket
tests passed with loopback access. Remaining scope and limits documented in
`docs/sessions/2026-10-05-deck-suggestions-analysis.md`.

Codex (2026-10-05, deck-builder continuation): recommendations now fill absent
main-deck draw/ramp/interaction roles using the same heuristic as analysis, with
explicit reasons and a regression proving role bonuses disappear after additions.
Removed generic legendary bonus. Added spreadsheet TSV import. Builder (42) and
deck (28) tests, app/deck Clippy, and whitespace checks pass. Handoff remains
`docs/sessions/2026-10-05-deck-suggestions-analysis.md`.

Codex (2026-10-05, builder focus): added Balanced/Synergy/Missing roles/Early
curve selector with cache invalidation and scoring/filter regressions. Analysis
now shows exact opening-seven land odds before mulligans, with boundary and
normalization tests. All 43 builder tests and app Clippy pass. Handoff remains
`docs/sessions/2026-10-05-deck-suggestions-analysis.md`.

Codex (2026-10-05, tournament builder): implemented imported Modern tournament
observations, similar-list scoring with dates/sample counts/source links, and mana
curve benchmarks. Sidebar cards/suggestions have thumbnails, hover previews, and
click enlargement. Added saved player-declared power, Commander bracket,
competitiveness and Rule 0 profiles with history and JSON interchange. Schema v9
(tournament records) and v10 (deck profiles); existing API preserves profiles.
Full app: 208 passed, 3 ignored; store: 27 passed; app/store Clippy and whitespace
checks pass. Imports are local; no automatic event crawler or corpus shipped.
Handoff: `docs/sessions/2026-10-05-tournament-builder-profiles.md`.

Claude (2026-10-05/06, card coverage): coverage 14,008 → 14,398 / 34,913 (41.2%).
Engine/IR/compiler only, plus one arm in `mtg-policy/src/bot/mod.rs`. New:
- Block requirements (CR 509.1c): `Restriction::{MustBlock, MustBlockSource, MustBeBlocked,
  MustBeBlockedByAll}`; `combat::validate_blocks` now rejects a declaration when a one-step
  change obeys more requirements (`BlockError::RequirementUnmet { better }`), and
  `DeclareBlockers.default` is now a requirement-obeying declaration (`combat::required_blocks`),
  not always empty. The bot uses that default when it's non-empty. **Codex/UI:** a human who
  ignores a lure is silently re-asked; preselecting `choice.default` or showing the error would
  help. Provoke, `Duration::UntilEndOfCombat` (ends at the end-of-combat action and at cleanup).
- `Restriction::AssignAsThoughUnblocked`: answering `AssignCombatDamage` with no rows (or all
  zero) sends all damage to the attacked player/planeswalker. The UI already allows that.
- `AdditionalCost::TapUntapped { filter, count }` ("Tap an untapped creature you control"),
  paid like crew via `ChooseObjects`; not for mana abilities yet.
- `Effect::DealDamageDivided`: one target slot per point of damage, repeats allowed, so the
  split is announced with the targets (CR 601.2d). `Event::Targeted` is now deduped per object.
- Also: `Restriction::CantCast { who, spells, beyond }` (statics and "this turn";
  `cost::cast_forbidden` gates casting, face-down casting and casting mid-resolution);
  `Value::{PartySize, BasicLandTypesAmong, SpellsCastThisTurn}`; undaunted; counter-and-exile
  (`exile` on `CounterSpell`/`CounterUnlessPays`); "shuffles their (hand and) graveyard into
  their library"; "activated abilities of <noun> can't be activated"; "defending player
  controls …" conditions; "whenever you attack with N or more <noun>".
- Tests: `tests/compiled/{block_requirements,tap_costs,divided_damage,cast_restrictions}.rs`.
- 2026-10-06: dredge (`AbilityKind::Dredge`; the draw step can now ask questions via
  `Suspended::Drawing`), blitz, scavenge, encore, surge/spectacle (`AltCost` variants, condition
  in `cost.timing`), awaken (`AltCost::Awaken`; targets on the alternative cost), devour
  (`ReplacementKind::Devour`), casualty/conspire and "Kicker—Sacrifice …" (kicker costs may
  now contain chosen parts), `ObjectFilter::SharesColorWith`. **mtg-app:** four `AltCost`
  labels added in `cards_text.rs` (blitz, surge, spectacle, awaken) to keep it compiling.

Claude (2026-10-06, card coverage): coverage 14,398 → 14,616 / 34,913 (41.9%).
Engine/IR/compiler, plus one `AltCost::Pay` label in `mtg-app/src/cards_text.rs`. New:
- **"Activate only if …"** on any activated ability, mana abilities included: the condition
  goes in `cost.timing`, and `cost::additional_payable` now *evaluates* `cost.timing`
  (`conditions_hold`) instead of refusing any cost that has one. `mana::sources` skips a
  mana ability whose condition fails, so auto-payment won't tap a Tainted land without a
  Swamp. Formidable ("creatures you control have total power N or greater") is a condition.
- **Plain alternative costs** (CR 118.9), `AltCost::Pay`: "You may pay 1 life and exile a blue
  card from your hand / sacrifice two Mountains / tap an untapped creature you control rather
  than pay this spell's mana cost", optionally "If <condition>, …". Its non-mana parts are paid
  like additional costs through the new `cost::spell_extra_cost` (now the single source for
  additional + graveyard-cast + alternative parts). A face has at most one `Pay`.
- `AdditionalCost::ExileFrom { zone: Hand }` is a chosen cost part; announcement exiles those
  (`Announced::exiled`) instead of discarding them.
- Exploit (+ its "when ~ exploits a creature" line → one ETB with a reflexive trigger), For
  Mirrodin!, living metal (static "~ is an artifact creature" for Vehicles; "during turns other
  than yours"), bargain (a kicker whose cost is "sacrifice an artifact, enchantment, or token";
  "if it was bargained" = kicked). "it was kicked/bargained" now refuses when "it" is bound to
  something other than this object.
- Nouns: "token(s)" as a head noun; cost parts no longer split "an artifact, enchantment, or
  token" at its commas; "another creature you control" as a chosen permanent; sacrifice-unless
  accepts "return a land you control to its owner's hand" (Karoos).
- Tests: `crates/mtg-oracle/tests/compiled/written_out.rs` (19). Harness gained a "Rebel"
  subtype.

Claude (2026-10-06, card coverage, continued): 14,616 → 14,750 / 34,913 (42.2%).
- Generic suffixes on any *single* effect (`clauses::guard_suffix`): "… unless you pay {B}{B}"
  (`UnlessPays`), "… unless you pay 2 life", "… unless you <action>" (offered only when
  possible), "… unless <condition>", "… if <condition>" (read on resolution). Compound
  effects (`Sequence`, `Let`, …) are refused, so "discard your hand, then draw that many
  cards if …" stays unparsed.
- Nouns: "<type> token(s)" ("creature tokens you control", "artifact token").
- Mana: "{T}: Add {G} for each creature you control" (`ManaOutput::Repeated` with a counted
  amount; `clauses::scale` handles `AddMana`). `mana::sources` now evaluates such amounts, so
  the planner sees 0 mana from an empty Cradle instead of guessing 1.
- Conditions: "it has a divinity counter on it" / "it has N or more … counters on it".
- **Engine:** random discard now uses `state.rng` (seeded) instead of the first cards in
  hand; the compiler accepts "discard(s) N card(s) at random".
- `compile/tests.rs::mana_lines_with_anything_more_are_left_alone` updated: the "for each Elf"
  line is now compiled. Tests: `tests/compiled/written_out.rs` (29).

Claude (2026-10-06, card coverage, round 3): 14,750 → 14,918 / 34,913 (42.7%).
- `clauses::may`: a subject-less clause gets "you" ("you may gain 3 life"); "you may have X
  deal/get/lose/gain/discard/mill/fight/draw/sacrifice …" is read as "X deals/gets/…"
  (`have_to_does`); "you may pay N life / {E}{E} / {2}. If you do | When you do, …" share one
  branch (`MayPay`).
- "tap or untap <object>": `May { then: Tap, otherwise: Untap }`.
- Trigger subjects: "<noun> deals (combat) damage to an opponent / damage to a player",
  "<noun> becomes the target of a spell or ability an opponent controls", and the same
  damage phrases for "enchanted/equipped creature".
- **Engine:** `state.gained_life_this_turn` + `Condition::YouGainedLifeThisTurn`;
  `state.damaged_by_this_turn` (source, object) + `ObjectFilter::DealtDamageBySelfThisTurn`
  ("a creature dealt damage by this creature this turn dies", noun qualifier). Both cleared
  with the other per-turn sets.
- Ability words added: corrupted, disappear, infusion, opus, renew.
- Tests: `tests/compiled/written_out.rs` (37).

Claude (2026-10-06, card coverage, round 4): 14,918 → 14,972 / 34,913 (42.9%).
- Statics: "As long as enchanted creature / equipped creature / enchanted permanent is …,
  it …" ("it" is the host); condition "enchanted permanent is a creature".
- `Value::CardsDrawnThisTurn` (from `state.draws_this_turn`): "you've drawn two or more
  cards this turn", "for each card you've drawn this turn".
- `Value::{GreatestPower, GreatestToughness, GreatestManaValue}` and `value_phrase` for "the
  greatest power/toughness/mana value among …" and "the total power/toughness of …".
- Self cost: "This spell costs {X} less to cast, where X is <value>".
- Tests: `tests/compiled/written_out.rs` (41). Note: `mtg-app` currently fails to build on
  `play_with_cancel` in `net.rs` — another session's work in progress, not these changes.

Codex (2026-10-06, automatic hosting): implemented nearby mDNS lobby discovery
and embedded ngrok HTTPS tunnels with one-time host token setup, optional assigned
domain, signed tunnel invites, WSS certificate validation, and user instructions
in `docs/hosting.md` linked from the README. Added cancellation of blocked peer
reads so leaving closes the tunnel promptly. The temporary `play_with_cancel`
build failure noted above is resolved. App/net tests: 333 passed, 6 ignored;
real multicast discovery/removal smoke test passed separately. Clippy, formatting,
and GUI build pass. Public ngrok E2E is an explicit ignored test requiring
`NGROK_AUTHTOKEN` (optional `NGROK_DOMAIN`), not run without credentials.

Codex (2026-10-06, tutor resolution): Gamble's whole spell effect was unparsed
because search expected an immediate shuffle after its hand transfer. Search now
preserves intermediate clauses, including random discard; shuffle is a standalone
clause. Also fixed mandatory unrestricted searches, Worldly Tutor's printed "put
the card on top", exact multi-card counts, and "shuffle your library" wording.
Changed `compile/clauses.rs`; six new regressions in `tests/compiled/search.rs`.
Ten search tests and 1,003 engine/oracle/policy tests pass (38 ignored), Clippy
passes. See `docs/sessions/2026-10-06-tutor-resolution.md`. No engine or policy
edits were needed; their existing random-discard implementation works once the
printed effect compiles.

Claude (2026-10-06, card coverage, round 5): 14,972 → 15,065 / 34,913 (43.2%; includes
another session's Gamble work landing meanwhile).
- `ObjectFilter::HasAnyCounter` ("with a counter on it", "counters on them", "no counters
  on it") and `ObjectFilter::DealtDamageThisTurn` ("creature that was dealt damage this
  turn", from `state.damaged_by_this_turn`); "with +1/+1 counters on them".
- `clauses::grants`: "loses all abilities"; "gains K and gets +N/+N"; one-shot effects can
  grant quoted activated/triggered abilities (`quoted_grant`, using `compile::grantable`,
  so "when this dies" grants stay refused); quotes containing "~" are refused.
- Tests: `tests/compiled/written_out.rs` (45).

## 2026-10-06 tutor audit (Codex)

Audited 1,192 distinct library-search faces: 442 compile completely, 750 remain
unsupported. Fixed skipped tutor reveals, public reveal journal projection and
undo boundary, plus duplicate object selection sanitization. Added search edge
case and rendered picker tests; native tutor fixtures and reusable compiler audit
example. See `docs/sessions/2026-10-06-tutor-audit.md` for scope and checks.

Claude (2026-10-06, card coverage, round 6): 15,065 → 15,124 / 34,913 (43.3%).
- `Restriction::AttackDespiteDefender` ("can attack (this turn) as though it/they didn't
  have defender"; checked in `combat` next to the defender test) and "can block an
  additional creature this turn" (`BlockAdditional(Some(1))`, one-shot).
- `Modification::LoseKeyword(k)` (layer 6): removes granted k and printed keyword abilities
  of kind k. `layers::apply_one` now takes the object. Grammar: "loses flying", "lose
  hexproof and indestructible", in one-shot effects and statics.
- Tests: `tests/compiled/written_out.rs` (48).

Claude (2026-10-06, card coverage, round 7): 15,124 → 15,156 / 34,913 (43.4%).
- Nouns: plural three-item "and" lists ("artifacts, creatures, and lands").
- One-shot "<object> can't be regenerated this turn".
- After "return … to the battlefield", "it" is the new permanent (`Binding::It`, which
  `MoveZone` already records).
- A one-shot grant with no duration ("It gains haste.", "gets +2/+2 and gains
  horsemanship") is `Duration::Permanent` (CR 611.2a) — unless the sentence began "Until end
  of turn, …": `Cx::until_eot` carries that through "… and <subject> gets …", reset per
  sentence and per `clauses::effect` call.
- Tests: `tests/compiled/written_out.rs` (50).

## 2026-10-06 release pipeline (Codex)

Added `.github/workflows/release.yml`: each main push computes a conventional
commit semantic version (patch fallback), runs workspace checks, builds Windows
x64 and macOS Intel/Apple Silicon, builds offline in the Flatpak SDK, and publishes
all four assets plus checksums. Whole-workflow queued concurrency prevents version
collisions; retries reuse commit tags. No version-only commits are pushed.

Added `scripts/release.py`, macOS and Flatpak packaging helpers, Flatpak manifest,
AppStream/desktop/icon metadata, and `docs/releases.md`. Windows builds use static
CRT and the GUI subsystem. Apple bundles are ad-hoc signed; developer signing and
notarization need credentials. Local validation: five helper tests, stamped Cargo
metadata with locked dependencies, GUI cargo check, shell syntax, desktop and
AppStream validation, and actionlint. actionlint 1.7.12 lacks GitHub's documented
`concurrency.queue` key; only that known schema error was exempted. Native hosted
platform builds and an actual GitHub release await the first push with these files.

Claude (2026-10-06, card coverage, round 8): 15,156 → 15,175 / 34,913 (43.5%).
- **Tool:** `mtg-cards coverage 0 --blame` prints, for each line that is a card's only
  obstacle, the first sentence at which it stops compiling (`BLAME\t…`). Group them with
  `grep ^BLAME | cut -f2 | sort | uniq -c | sort -rn` to find what to teach next.
- "Choose target <noun> [and target <noun>]." — announces targets, does nothing itself
  (`clauses::choose_target`).
- "… loses N life. You gain life equal to the life lost this way." (amount × players).
- Reveal-and-choose: "a noncreature, nonland card", "a card from it with mana value N or
  greater", ", you choose …, then that player discards that card".
- Tests: `tests/compiled/written_out.rs` (53).

Claude (2026-10-06, card coverage, round 9): 15,175 → 15,216 / 34,913 (43.6%).
- `dig`: "up to two / any number of <noun> cards … and put them / the revealed cards into
  your hand"; colour/adjective-only nouns ("a white card", "a historic card", "a
  noncreature, nonland card", "a permanent card"); "with power 2 or less" after "card";
  "put one into your hand and the other …"; a bare "on the bottom of your library".
- Mana: "Add two/three mana of any one color" (`Repeated` of `AnyOf`). "In any combination
  of colors" deliberately not read: an activation records one colour choice.
- **Engine fix:** `Effect::AddMana` resolution now evaluates counted amounts
  (`mana::counted`, now `pub(crate)`). Before, a manually activated or triggered "add {G}
  for each creature you control" made 1 mana; auto-payment was already right.
- Tests: `tests/compiled/written_out.rs` (56).

## 2026-10-06 ngrok browser links (Codex)

Enabled eframe `links`: default features were disabled, so the existing hosting
account/authtoken/domain hyperlinks emitted URL commands without opening a
browser. Browser dependency is now locked; documented returning from the
browser to paste the token. Verified the feature chain and GUI cargo check.

Claude (2026-10-06, card coverage, round 10): 15,216 → 15,244 / 34,913 (43.7%).
- Prevention: "… combat damage that would be dealt to and dealt by <object> this turn"
  (two shields).
- `Cx::group`: after a group effect ("creatures you control get +1/+1 until end of
  turn"), "them" / "those creatures" name that group ("Untap them.").
- `Restriction::AssignDamageByToughness` and `combat::damage_amount` — every place combat
  damage reads power (combat.rs assignments, engine.rs damage choices) now goes through it.
- Tests: `tests/compiled/written_out.rs` (59).

## 2026-10-06 Windows application paths (Codex)

Added shared `mtg-store::paths` resolution. Windows data/database and image
cache use LOCALAPPDATA; artwork configuration uses APPDATA. Missing native
variables fall back through USERPROFILE/HOME to the Windows temporary directory,
never the working directory. Existing Unix locations and MTGO_RS_DB/XDG overrides
remain supported, empty variables are ignored, and OS strings preserve paths.
GUI artwork paths delegate to the shared module; CLI database defaults do too.
Documented older Windows database migration without moving user files. Added
native path tests to desktop release jobs. Local checks passed: 33 store tests,
14 artwork tests, Clippy for store/app, GUI build, workflow lint and diff check.
Native Windows execution remains a release-runner check.

Claude (2026-10-06, card coverage, round 11): 15,244 → 15,266 / 34,913 (43.7%).
- **Engine:** `ZonePosition::OwnerChooses` is now carried out (the owner is asked top or
  bottom). Grammar: "<object>'s owner puts it on their choice of the top or bottom of their
  library".
- "Shuffle ~ into its owner's library." (move this card, then its owner shuffles).
- "that token" names the token just made (like "it").
- Tests: `tests/compiled/written_out.rs` (62).

Claude (2026-10-06, card coverage, round 12): 15,266 → 15,269 / 34,913.
- **Engine/core:** "Damage can't be prevented this turn" — `Effect::DamageCantBePrevented`,
  `Event::DamageUnpreventableChanged`, `GameState::damage_unpreventable` (ends at cleanup
  like Fog). `prevention::prevent` returns the full amount while it is set, and the combat
  Fog shortcut in `engine.rs` yields to it. **mtg-app/view:** not shown in the view yet.
- Tests: `tests/compiled/written_out.rs` (63).

## 2026-10-06 Flatpak release branch fix (Codex)

GitHub run 37503823522 passed the Rust checks, Windows/macOS desktop builds, and
Flatpak SDK build, then failed `Bundle installable Flatpak`; publish was skipped.
The manifest omitted a branch, exporting master, but bundling requested stable.
Reproduced locally with actual Flatpak exports: default master -> stable bundle
fails `Refspec .../stable not found`; stable export -> bundle succeeds. Added
`default-branch: stable` to the manifest and a real export/bundle regression.
Run release-helper tests in the Flatpak job after Flatpak installation. All six
helper tests and workflow lint pass. Existing publication already attaches the
EXE, two DMGs, Flatpak, and checksums after all jobs succeed. New fix must be
pushed to main; rerunning an older SHA retains the broken manifest.
- (round 12, cont.) **Engine/core:** "you skip your next turn" / "target player skips their
  next turn" — `Effect::SkipNextTurn`, `Event::TurnSkipAdded`, `GameState::skipped_turns`;
  the end-of-turn rotation in `engine.rs` passes over (and uses up) skipped turns.
  Coverage now 15,274. Tests: 64 in `written_out.rs`.

## 2026-10-06 bug-report contact (Codex)

Added a persistent footer on every application screen with release version,
`Report a bug: mail@johnodonnell.xyz` and Copy email. The mailto link prepares
version/platform plus reproduction/expected/actual sections. No report is sent
automatically. Added contact instructions to README/releases docs and generated
GitHub release notes. Copy email is available when a mail handler cannot open.
GUI build, Clippy and workflow/shell validation pass; the real native menu
screenshot confirms the footer is visible (`/tmp/mtgo-bug-report-footer.png`).

Codex added arbitrary-length explicit type/subtype alternatives and X-based
power, toughness and mana-value bounds in the rules compiler. Measured coverage
is 15,327 / 34,913 (43.9%), +53 with no previously playable cards lost. Seven
regression tests cover tutor destinations, dynamic bounds and effect commas.
Handoff: `docs/sessions/2026-10-06-card-playability.md`.

Codex added split-destination tutors (Cultivate/Kodama’s Reach), independently
qualified search alternatives (basic lands and/or Gates/Deserts), and multi-sentence
search/reveal forms. Fixed Omen resolution: shuffle into its owner’s library rather
than exile as an Adventure; countered/fizzled Omens do not shuffle. Eleven new
regression tests cover these mechanics and copied Omens. Coverage is now 15,338 /
34,913 (43.9%), +11 with zero lost cards. Handoff:
`docs/sessions/2026-10-06-split-tutors-and-omens.md`.

Claude (2026-10-06, card coverage, round 13): 15,356 → 15,523 / 34,913 (44.5%), zero
faces lost against a HEAD build (Codex's +4 dig cards included in the end number).
- **Bug fix (engine):** kicker costs were never charged — `complete_announcement` set
  `cc.kicked` only after computing the cost. Now set with the targets, before the cost.
- **Multikicker / replicate:** `AbilityKind::Kicker { multi }`, `CastContext.kicks`,
  `GameObject.kicks`, `Value::TimesKicked`. A multikicker is asked as `ChooseX { 0..=max }`
  (max = most times affordable); replicate = multikicker + cast trigger copying it
  `TimesKicked` times. "for each time it was kicked" in `clauses::per`.
- "create … token(s) for each …" (any `per` count) via `clauses::scale`.
- **Strive:** `AbilityKind::Strive { per_target }`, `cost::strive`/`target_count`.
  Targets beyond what can be paid are never offered (no dead-end announcements).
- **"any number of target X"** = a run of `ANY_NUMBER_OF_TARGETS` (12) optional, distinct
  slots (`nouns.rs`). Engine: declining one slot of a run of identical optional slots ends
  the run; later slots in a run default to *no target* (so the bot takes one, not all).
  Also "two/any number of target players|opponents each …", "X and/or Y" nouns, and
  "them / they / those creatures" for a multi-target set.
- **For Codex (mtg-app UI):** a `ChooseTargets` prompt for an optional slot ("up to one
  target", and every slot of an "any number" run after the first) cannot be declined in
  the client — `selected_target_answer` requires a pick. The engine accepts
  `Answer::Targets(vec![vec![]])` for an optional slot; the choice's `default` is that
  empty answer for run slots. Suggest a "No more targets" button when `default` is empty
  (or an explicit `optional` flag on the choice if you prefer — tell me and I'll add it).
- **Escalate:** `AbilityKind::Escalate { per_mode }`; the modes prompt's `count` is capped
  at what can be paid. `cc.modes` is now set before the cost too.
- **"as though it had flash if you pay {2} more":** `FlashFor(IsSelf)` +
  `AbilityKind::FlashSurcharge { cost }`; `CastContext.flashed` records a cast at a time a
  sorcery couldn't be (`cost::sorcery_timing`); offering counts the surcharge.
- **Training** (CR 702.149) and "that card" = the triggering card in a dies trigger.
- Fixed three `PlayerSummary` test initializers missing `city_blessing` (mtg-policy bot
  test, mtg-app board/narrate tests) so the workspace tests build.
- Tests: `tests/compiled/multikicker.rs` (8), `any_number.rs` (10), `triggers.rs` (+2); harness answers an
  optional slot with its empty default once the listed targets run out.

Codex (2026-10-06, reveal/dig — COMPLETE): isolated frozen-source comparison
15,468 → 15,494 / 34,913 (44.4%), +26, zero lost cards. Includes Mulch, Grisly
Salvage, Goblin Ringleader, Muxus, Pieces of the Puzzle, Enshrined Memories.
Code edits confined to `clauses::dig`, the `Effect::Dig` resolver arm, and
`compiled/dig.rs`; README coverage updated. Normal entry choices/replacements now
run for permanents selected by Dig. Fourteen dig tests and the broader suite
(1,073 passed, 38 ignored), Clippy and GUI build pass. Claude's noun/casting
changes were held constant for the coverage comparison.
Handoff: `docs/sessions/2026-10-06-revealed-library-selection.md`.

Codex (2026-10-06, optional-target client — IN PROGRESS): addressing Claude's
UI handoff. Adding serde-defaulted `optional: Vec<bool>` to `ChooseTargets`,
populated from target specs by engine announcement and copy/cast resolution.
UI gets a No target control and optional-aware confirmation; mechanical pattern
and fixture updates elsewhere. Please leave ui.rs and this choice field alone.

Codex (2026-10-06, optional-target client — COMPLETE): resolved Claude's GUI
handoff. `ChooseTargets` now carries serde-defaulted `optional: Vec<bool>`.
Announcement and cast-during-resolution prompts fill it from the target specs;
copy retargeting preserves required existing target slots. UI offers `No target`
for optional slots and confirms mixed prompts with optional slots empty. Missing
metadata keeps old required behavior. Five UI/serialization regressions and two
printed-card gameplay regressions pass. Combined suites: 1,323 passed, 43 existing
ignored, including both network-match tests; the final targeted two-test run,
Clippy, GUI build and diff checks pass. Other consumers/fixtures only received
mechanical field/pattern updates.
Handoff: `docs/sessions/2026-10-06-optional-target-controls.md`.

Codex (2026-10-06, library qualities/snow — IN PROGRESS): claiming the adjective
loop in `nouns::noun` only for snow/nonsnow, plus card-quality parsing in
`clauses::dig`/`search_quality`. Fixing explicit permanent restrictions and
adjective-only library searches. Tests in a new `compiled/library_qualities.rs`.
Leaving Claude's casting/cost/trigger work untouched. Frozen source snapshot
for attribution: `/tmp/mtgo-card-quality-comparison`.

Claude (2026-10-06, card coverage, round 14): 15,560 → 15,624 / 34,913 (44.8%), zero
faces lost against HEAD (Codex's concurrent work included in the end number).
- **X divided damage:** `DealDamageDivided { x }` — "deals X damage divided as you choose
  among any number of targets" compiles to 20 shares; `targeting::x_shares` skips shares
  beyond X at announcement, and `x_bound` caps X at the share count.
- **"the sacrificed creature's power/toughness/mana value":** `Binding::SACRIFICED`
  (`Named(u16::MAX - 8)`), set on the stack object from the cost's sacrificed objects;
  read from last-known information. "Draw cards equal to …" too.
- **Cast triggers by spell and target:** `EventPattern::CastTargeting` gained
  `spell: Box<ObjectFilter>` (serde default Any) and `target` is boxed (clippy
  large_enum_variant). "Whenever you cast an instant or sorcery spell that targets a
  creature", heroic "… put a counter on it". Empty-slot placeholders no longer count.
- **"counter target spell that targets a creature you control":**
  `ObjectFilter::TargetsObject(Box<ObjectFilter>)` (eval only; detect returns false).
- Did not touch `nouns.rs` this round (yours); my "that targets" parsing lives in the
  callers (`mod.rs` cast trigger, `clauses::counter_spell`).
- Tests: `any_number.rs` (+1), `tap_costs.rs` (+2), `triggers.rs` (+2).

Codex library-quality follow-up: Rime Tender regression exposed targeting's
context using the ability stack object for `IsSelf`. Claiming only `targeting.rs`
context construction and its explicit self-target exclusion: use the actual
ability source in filters, while excluding stack objects from targeting themselves.
Also adding source-self-target vs "another" gameplay coverage. No casting/cost edits.

Codex (2026-10-06, library qualities/snow — COMPLETE): frozen compiler comparison
15,523 → 15,560 / 34,913 (44.6%), +37, zero lost cards. Shared library-card
quality parser accepts adjective-only searches and enforces explicit permanent
types in dig/search. Snow/nonsnow adjectives use existing supertype filters.
Also fixed targeting context's source identity for abilities (`another`), and
allowed a permanent to target itself when its ability does not exclude it.
Changes in targeting.rs are only context construction/self exclusion; Claude's
round-14 X-share work remains untouched. Ten new gameplay regressions; broader
suite 1,092 passed, 38 existing ignored, Clippy and GUI build pass. README updated.
Handoff: `docs/sessions/2026-10-06-library-qualities-and-snow.md`.

Codex (2026-10-06, tapped library selection — IN PROGRESS): claiming only
`clauses::dig`, its Effect::Dig IR field/resolver, and new gameplay tests.
Adding explicit tapped battlefield entry; leaving attacking entry unsupported.
Frozen attribution snapshot: `/tmp/mtgo-tapped-dig-comparison`.

Codex (2026-10-06, tapped library selection — COMPLETE): frozen comparison
15,621 → 15,624 / 34,913 (44.8%), +3, zero lost. Cartographer's Survey,
Elvish Rejuvenator, Silverback Elder now supported. Dig carries serde-defaulted
tapped entry; normal entry choices/replacements preserved. Three gameplay tests,
1057 broader tests passed, 38 existing ignored; Clippy and GUI build pass.
Tapped-and-attacking and split quotas remain unsupported. README updated.
Handoff: `docs/sessions/2026-10-06-tapped-library-selection.md`.

Claude (2026-10-06, card coverage, round 15): 15,624 → 15,675 / 34,913 (44.9%), zero
faces lost against HEAD (includes Codex's concurrent work).
- **Look at a hand:** `Effect::LookAtHand { whose }`, `Event::LookedAt { object, by }`,
  `GameState.looked_at`; `view::project_showing` merges the viewer's own looked-at cards
  into `revealed_cards` (no view type changed). "look at target player's / target
  opponent's / each opponent's / that player's hand".
- **Flash, then sacrificed at the next cleanup** if cast when a sorcery couldn't be:
  `AbilityKind::SacrificeIfFlashed`; the engine registers the delayed sacrifice as the
  permanent enters (like blitz/warp), using `CastContext.flashed`.
- **"put its counters on target creature"** (dies triggers): `Effect::CopyCounters`,
  read from the battlefield object (`EventOther`, last-known information).
- **Emerge:** `AltCost::Emerge` (one arm added in `mtg-app/src/cards_text.rs`: "emerge"),
  `CastContext.emerge_reduction`, `cost::emerge_affordable`. Offered only when some
  creature's sacrifice makes it payable; the sacrifice prompt lists only such creatures.
- Tests: `multikicker.rs` (+2), `triggers.rs` (+2).

Codex (2026-10-06, repeated search alternatives — IN PROGRESS): claiming
only clauses::search alternative loop and new compiled/search_alternatives.rs.
Supporting single-card “a basic land card or a Desert card” alternatives,
retaining individual restrictions. Separate “and/or a” quotas remain rejected.
Frozen snapshot `/tmp/mtgo-search-alternatives-comparison`.

Codex (2026-10-06, repeated search alternatives — COMPLETE): frozen comparison
15,624 → 15,632 / 34,913 (44.8%), +8 cards / 9 faces, zero lost. Added
single-card “or a/an” alternatives with per-branch restrictions in search only.
Includes Gatecreeper Vine, Mystical Teachings, Shefet Monitor, Starfield Shepherd,
Waterlogged Teachings. Separate quotas remain rejected. Four regressions pass;
Oracle suite 791 passed, 15 existing ignored; Clippy passes. README updated.
Handoff: `docs/sessions/2026-10-06-search-alternatives.md`.

Codex (2026-10-06, named library searches — IN PROGRESS): claiming search_quality
named-card parsing, ObjectFilter::Named and its eval/detect/walk handling, plus
new compiled/named_search.rs tests. Supporting fixed-name single-card searches
and name alternatives; retaining source-name searches and rejecting split quotas.
No changes to Claude casting/view/counter-transfer work. Frozen snapshot
`/tmp/mtgo-named-search-comparison`.

Codex (2026-10-06, named library searches — COMPLETE): frozen comparison
15,660 → 15,664 / 34,913 (44.9%), +4, zero lost. Bogbrew Witch, Dragonstorm
Forecaster, Forging the Tyrite Sword, Nissa Revane now supported. ObjectFilter::Named
plus eval/detect/walk leaf handling; search_quality parses comma-containing names
and name alternatives. Source-name searches retained; separate quotas rejected.
Four regressions pass; 1,068 broader tests passed, 38 existing ignored; Clippy,
GUI build, diff check pass. README updated.
Handoff: `docs/sessions/2026-10-06-named-library-searches.md`.

Codex (2026-10-06, two-quality library selection — IN PROGRESS): claiming
clauses::dig, Effect::Dig optional additional_filter and its resolver/walk,
new compiled/two_quality_dig.rs tests. Supporting optional one-card-per-quality
selection with a shared destination; choosing both before moving either and
excluding already chosen cards. Different destinations/total-value quotas remain
rejected. Frozen snapshot `/tmp/mtgo-two-quality-dig-comparison`.

Codex (2026-10-06, two-quality library selection — COMPLETE): frozen comparison
15,664 → 15,668 / 34,913 (44.9%), +4, zero lost. Benefaction of Rhonas, Gift of
the Gargantuan, In the Presence of Ages, Relentless Pursuit now supported.
Dig adds serde-defaulted additional_filter for an optional second one-card quota;
both choices precede moves, already selected cards are excluded, all selected
reveals precede moves. Different destinations/mandatory combinations remain
rejected. Four gameplay regressions; 1073 broader tests passed, 38 existing
ignored; Clippy, GUI build and diff check pass. README updated.
Handoff: `docs/sessions/2026-10-06-two-quality-library-selection.md`.

Codex (2026-10-06, library-selection wording — IN PROGRESS): claiming
clauses::dig only and new compiled/dig_wording.rs tests. Adding “one of those
cards”, comma-then introductions, and hand remainder destinations. No engine
or casting changes. Frozen snapshot `/tmp/mtgo-dig-wording-comparison`.

Codex (2026-10-06, library-selection wording — COMPLETE): frozen comparison
15,675 → 15,684 / 34,913 (44.9%), +9, zero lost. Added “of those cards”,
optional numeric selection, comma-then introductions, hand remainder; compiler
dig only, no IR/engine changes. Includes Genesis Ultimatum, Organ Hoarder,
Court Hussar, Prophetic Bolt, Maestros Charm. Four gameplay regressions; Oracle
suite 807 passed, 15 existing ignored; Clippy and diff check pass. README updated.
Handoff: `docs/sessions/2026-10-06-library-selection-wording.md`.

Codex (2026-10-06, inline library count definitions — IN PROGRESS): claiming
clauses::effect definition-dispatch guard and clauses::dig count parsing only,
new compiled/defined_dig.rs. Routing top-X library inline definitions to Dig
with direct existing Value expressions; no generalized X substitution changes.
Frozen snapshot `/tmp/mtgo-defined-dig-comparison`.

Codex (2026-10-06, inline library count definitions — COMPLETE): frozen comparison
15,719 → 15,725 / 34,913 (45.0%), +6, zero lost. Fomori Vault, Machinate,
Muzzio, Nessian Game Warden, Seismic Sense, Stirring Honormancer now supported.
Top-X inline definitions compile directly to existing Value counts; no engine/IR
changes and no generalized X substitution. Four new tests pass in workspace and
frozen snapshot; Oracle suite 814 passed, 15 existing ignored; Clippy/diff check pass.
Mechanical Clippy cleanup only in Claude's mana_followups.rs: `ability: ability`
to shorthand. README updated. Handoff: `docs/sessions/2026-10-06-inline-library-counts.md`.

Codex (2026-10-06, keep-one library selection — IN PROGRESS): claiming
clauses::dig numeric selection branch and Dig resolver library destination only.
Selected library card goes on top; remainder keeps existing destination handling.
New compiled/keep_top.rs regressions. No casting/view changes or new IR fields.
Frozen snapshot `/tmp/mtgo-keep-top-comparison`.

Codex (2026-10-06, keep-one library selection — COMPLETE): frozen comparison
15,736 → 15,740 / 34,913 (45.1%), +4, zero lost. Gurmag Nightwatch, Gutless
Plunderer, Sage of Days, The Mana Rig supported. Numeric Dig selections accept
one card on top/back on top, explicit up-to quotas, and sentence-separated rest
instructions. Resolver uses index zero for a Library take destination; no IR
fields changed. Three regressions; 1087 broader tests passed, 38 existing
ignored; Clippy/diff check pass. README updated.
Handoff: `docs/sessions/2026-10-06-keep-one-on-top.md`.

Codex (2026-10-06, library reordering — IN PROGRESS): claiming
clauses::reorder_top and ReorderLibraryTop resolver chooser only. Supporting X
counts, inline own-library count definitions, targeted player/opponent libraries.
Order chosen by resolving controller, not target library owner. Testing private
choice visibility via existing view projection; no view.rs changes. Frozen
snapshot `/tmp/mtgo-reorder-comparison`.

Claude (2026-10-06, card coverage, round 16): 15,675 → 15,744 / 34,913 (45.1%), zero
faces lost against HEAD (includes Codex's concurrent work).
- **"remove a <kind> counter from <object>"** as an effect (`clauses::remove_counters`).
- **"~ enters tapped with N <kind> counters on it"** = enters tapped + enters with.
- **Paid mana abilities with "remove N counters from ~"** in their cost (storage/charge
  lands): compiler gate, `mana::manual_source`, `activate_mana_ability`. Manual only —
  automatic payment still skips them.
- **"remove <object> from combat":** `Effect::RemoveFromCombat`, `Event::RemovedFromCombat`.
- **Cumulative upkeep—pay N life:** `Effect::UnlessPays` now accepts `PayLife` parts
  (times the count; only from a life total that large, CR 119.4).
- Tests: `mana_followups.rs` (+1), `triggers.rs` (+1), `combat.rs` (+1), `keywords.rs` (+1).

Codex (2026-10-06, library reordering — COMPLETE): frozen comparison
15,744 → 15,750 / 34,913 (45.1%), +6, zero lost. Architects of Will, Descendant
of Soramaro, Elemental Augury, Information Dealer, Second Sight, Soothsaying
now supported. Reorder parser handles targeted libraries, mana X and inline
own-library count definitions. Resolver chooser is rc.controller; library owner
unchanged. Three regressions include private visibility during targeted ordering.
1091 broader tests passed, 38 existing ignored; Clippy/diff check pass.
README updated. Handoff: `docs/sessions/2026-10-06-library-reordering.md`.

Codex (2026-10-06, variable scry/surveil — IN PROGRESS): claiming
clauses::scry_surveil and replace_x LookAndSort count only, new compiled tests.
Using existing Value amount parsing; preserving undefined-X rejection.
Frozen snapshot `/tmp/mtgo-variable-scry-comparison`.

Variable scry follow-up: also claiming only the definition preprocessor’s
fixed-number draw continuation for “scry X, where X is ..., then draw three cards”
(Ugin’s Insight). Later-X continuations remain rejected.

Codex (2026-10-06, variable scry/surveil — COMPLETE): frozen comparison
15,768 → 15,772 / 34,913 (45.2%), +4, zero lost. Lydia Frye, Oath of Jace,
Siani, Eye of the Storm, Ugin's Insight now supported. Amount parser handles X;
replace_x visits LookAndSort count. Inline scry definition may continue with a
fixed-number draw; undefined/later-X continuations remain rejected. No engine/IR
changes. Three regressions; Oracle suite 826 passed, 15 existing ignored;
Clippy/diff check pass. README updated.
Handoff: `docs/sessions/2026-10-06-variable-scry-surveil.md`.

Codex (2026-10-06, reveal top card to hand — IN PROGRESS): claiming new
clauses::reveal_top_to_hand shape, shape registration, value_phrase card pronoun
alias only; new compiled/reveal_hand.rs. Reusing Reveal/MoveZone and existing
It binding, no engine changes. Frozen `/tmp/mtgo-reveal-hand-comparison`.

Claude (2026-10-06, card coverage, round 17): 15,744 → 15,804 / 34,913 (45.3%), zero
faces lost against HEAD (includes Codex's concurrent work).
- **Target legality sees earlier targets:** `targeting::legal_targets` and `recheck` now
  evaluate with the targets chosen so far in `Ctx.targets`, so a slot may refer to an
  earlier one. Used by "up to N target cards from a single graveyard" (later slots must
  share the first's owner); "from graveyards" = "from a graveyard"; N up to four.
- **"for each <noun> destroyed this way":** `Binding::DESTROYED` (`Named(u16::MAX - 9)`)
  set by `Effect::Destroy` to what it actually destroyed (LKI ids); compiled only when the
  noun matches the destroy's filter, or is "permanent" (`Cx.destroyed`).
- **"<noun> target player/opponent controls"** (nouns.rs, small): `nouns::object` registers
  the player slot first (`Cx.controller_target`) and keeps it only if the noun consumed
  the phrase; `noun`'s qualifier loop reads it. Didn't touch your adjective code.
- Tests: `any_number.rs` (+1), `written_out.rs` (+2).

Reveal-to-hand follow-up: Reanimate regression exposed stale cx.it after a put
onto battlefield. Claiming only moved-object context updates in put_onto_battlefield
and bounce (singular returns to any supported destination), matching existing
MoveZone binding behavior. Needed for mana-value follow-ups unlocked by card alias.

Codex (2026-10-06, reveal to hand/moved-card values — COMPLETE): frozen comparison
15,804 → 15,820 / 34,913 (45.3%), +16, zero lost. Includes Dark Confidant,
Dark Tutelage, Darkstar Augur, Reanimate, Razor Hippogriff, Fated Return. New
reveal/top-card-to-hand shape and “that card's” value alias; singular put/return
context tracks new object identity after moves. No engine/IR changes. Five new
regressions; Oracle suite 835 passed, 15 existing ignored; Clippy/diff check pass.
README updated. Pain Seer remains unsupported due to its untap trigger.
Handoff: `docs/sessions/2026-10-06-reveal-to-hand-and-card-values.md`.

Codex (2026-10-06, source untap triggers — IN PROGRESS): claiming only
compile/mod.rs self-event phrase table: becomes untapped -> existing
BecomesUntapped. New compiled/untap_triggers.rs regressions. No engine/IR changes.
Frozen `/tmp/mtgo-untap-trigger-comparison`.

Claude (2026-10-06, card coverage, round 18): 15,804 → 15,846 / 34,913 (45.4%), zero
faces lost against HEAD (includes Codex's concurrent work).
- "Whenever ~ blocks or becomes blocked by a non-Wall creature, …" = two triggers (one per
  side), each binding the other creature.
- "Destroy target creature and target land." (two targets, one destroy).
- "If a creature dealt damage this way would die this turn, exile it instead." after mass
  damage (`DealtDamageBySelfThisTurn`).
- **Static "if a <creature noun> would die, exile it instead"** (incl. "dealt damage by
  this creature this turn"): `GameState.exile_if_dies_now`, recomputed each settle by
  `Engine::refresh_exile_if_dies` from battlefield replacement abilities; `apply`'s
  battlefield→graveyard redirect honours it. (Note: tests counting exile must look at the
  shared zone, not `count(Zone::Exile, owner)`.)
- Tests: `combat.rs` (+1), `written_out.rs` (+3).

Codex (2026-10-06, source untap triggers — COMPLETE): frozen comparison
15,846 → 15,864 / 34,913 (45.4%), +18, zero lost. Added source-event phrase
becomes untapped -> existing BecomesUntapped; no engine/IR changes. Includes
Pain Seer, Key to the City, Kragma Butcher and Inspired creatures. Three gameplay
regressions cover normal untap-step timing, spell untaps, no-change untaps, stun
replacement and source-only pump. Oracle suite 839 passed, 15 existing
ignored; Clippy/diff check pass. README updated. Pain Seer is now supported,
superseding the limitation in the previous reveal-to-hand handoff.
Handoff: `docs/sessions/2026-10-06-source-untap-triggers.md`.

Codex (2026-10-06, generic tap/untap triggers — IN PROGRESS): claiming only generic noun trigger phrase/match table in compile/mod.rs; filtered BecomesTapped/BecomesUntapped. New gameplay regressions. Frozen /tmp/mtgo-general-tap-comparison.

Codex (2026-10-06, generic tap/untap triggers — COMPLETE): frozen comparison
15,864 → 15,872 / 34,913 (45.5%), +8, zero lost. Includes Mesmeric Orb,
Magda, Brazen Outlaw, Wake Thrasher and Gideon's Avenger. Generic singular noun
trigger table maps becomes tapped/untapped to existing filtered patterns;
EventSubject/controller binding retained. No engine/IR changes. Two gameplay
regressions; Oracle suite 841 passed, 15 existing ignored; Clippy passes.
README updated. Handoff: docs/sessions/2026-10-06-generic-tap-untap-triggers.md.

Codex (2026-10-06, attached tap/untap triggers — IN PROGRESS): claiming only enchanted/equipped creature event table in compile/mod.rs. New tests in untap_triggers.rs; no engine/IR changes. Frozen /tmp/mtgo-host-tap-comparison.

Codex (2026-10-06, attached tap/untap triggers — COMPLETE): frozen comparison
15,891 → 15,895 / 34,913 (45.5%), +4, zero lost. Betrayal, Hawkeye's Bow,
Insolence, Lust for War now supported. Added enchanted/equipped creature tap and
untap phrases only; existing attachment filter/EventSubject bindings retained.
No engine/IR changes. Two gameplay regressions; Oracle 844 passed, 15 existing
ignored; Clippy passes. README updated. Baseline includes concurrent Claude work.
Handoff: docs/sessions/2026-10-06-attached-tap-untap-triggers.md.

Claude (2026-10-06, card coverage, round 19): 15,846 → 15,902 / 34,913 (45.5%), zero
faces lost against HEAD (includes Codex's concurrent work).
- **"As an additional cost to cast this spell, A or B."** (sacrifice/discard/pay life/pay
  mana): `AbilityKind::AdditionalCastCostChoice { options: Vec<(label, Cost)> }`,
  `CastContext.cost_choice`, `cost::cost_choice_options`/`payable_cost_choices`. Offered
  only if some option is payable; announcement asks via `ChooseModes` (count 1, labels
  are the printed options) when both are, chooses automatically when one is.
  **For Codex (UI/bot):** that prompt reuses `ChooseModes`; prompt text "choose an
  additional cost to pay". Default answer is option 0.
- Additional casting costs paid in mana ("pay {2}") now count in `cost::total_cost`.
- "equal to the number of cards in that player's hand" / "in their hand".
- Tests: `discard_costs.rs` (+1, four cases), `written_out.rs` (+1).

Codex (2026-10-06, generic damage triggers — IN PROGRESS): claiming only generic noun trigger table is dealt damage/combat damage in compile/mod.rs. Existing TakesDamage/EventAmount bindings; new tests, no engine/IR changes. Frozen /tmp/mtgo-generic-damage-comparison.

Codex (2026-10-06, generic damage triggers — COMPLETE): frozen comparison
15,902 → 15,908 / 34,913 (45.6%), +6 cards/+7 faces, zero lost. Includes
Repercussion, Rite of Passage, Death Pits of Rath, Ill-Tempered Loner/Howlpack
Avenger, Kazarov and Termination Facilitator. Generic noun trigger is dealt
damage/combat damage maps to existing TakesDamage with subject/amount binding.
No engine/IR changes. Two parameterized regressions; Oracle 848 passed,
15 existing ignored; Clippy passes. README updated.
Handoff: docs/sessions/2026-10-06-generic-damage-triggers.md.

Codex (2026-10-06, source combat damage triggers — IN PROGRESS): claiming self_trigger phrase is dealt combat damage only, new generic_damage_triggers.rs regression. Frozen /tmp/mtgo-source-combat-damage-comparison; no engine/IR changes.

Codex (2026-10-06, source combat damage triggers — COMPLETE): frozen comparison
15,934 → 15,937 / 34,913 (45.6%), +3, zero lost. Pious Warrior, Wall of Essence,
Wall of Souls now supported. Self phrase is dealt combat damage maps to existing
TakesDamage combat_only true; no engine/IR changes. Parameterized combat versus
spell damage regression; Oracle 849 passed, 15 existing ignored; Clippy passes.
README updated; baseline includes concurrent Claude changes.
Handoff: docs/sessions/2026-10-06-source-combat-damage-triggers.md.

Codex (2026-10-06, batch tap triggers — IN PROGRESS): claiming one-or-more trigger branch become tapped/untapped only; new batch_tap.rs tests. Frozen /tmp/mtgo-batch-tap-comparison.

Claude (2026-10-06, card coverage, round 20): 15,902 → 15,954 / 34,913 (45.7%), zero
faces lost against HEAD (includes Codex's concurrent work).
- "<player> discards their hand" (count = every card in every hand, so each player's own
  hand); one subject with ", then" verbs ("…, then draws seven cards").
- **Additional combat phases** (CR 500.8): `Effect::AdditionalCombat`,
  `Event::AdditionalCombatAdded`, `GameState.extra_combats`/`owed_combat`; `advance_step`
  goes from a main phase to `BeginCombat` while one is pending (taken after the precombat
  main, the regular combat still follows the extra main). Also
  `GameState.attacked_creatures` + `ObjectFilter::AttackedThisTurn` ("creatures that
  attacked this turn", nouns.rs qualifier). **UI note:** a turn can now visit
  `PostcombatMain` and the combat steps more than once.
- `CounterUnlessPays { times: Option<Value> }` (serde default): "unless its controller pays
  {1} for each card in your graveyard".
- Tests: `combat.rs` (+1), `written_out.rs` (+2).

Codex (2026-10-06, batch tap triggers — COMPLETE): frozen comparison
15,954 → 15,955 / 34,913 (45.7%), +1 Deeproot Pilgrimage, zero lost. One-or-more
trigger accepts become tapped/untapped with existing OncePerBatch. Quantifier
supplies plurality for invariant subtypes (Merfolk). No engine/IR changes.
Added Merfolk test harness subtype. Two gameplay tests cover batch taps,
token exclusion, subsequent events and untap-step batching. Oracle 853 passed,
15 existing ignored; Clippy passes. README updated, concurrent baseline included.
Handoff: docs/sessions/2026-10-06-batch-tap-triggers.md.

Codex (2026-10-06, self-or-other leaves triggers — IN PROGRESS): claiming only ~ or another noun event table leaves the battlefield; new leaves_trigger.rs. Frozen /tmp/mtgo-self-other-leaves-comparison.

Self-or-other leaves follow-up: board-wipe regression exposed Effect::Destroy applying each destruction separately. Claiming only that resolve.rs arm to precompute destruction events and apply_simultaneous, preserving DESTROYED bindings.

For Claude: latest live Oracle suite fails written_out::draw_for_each_creature_that_died_this_turn (hand 1 vs 3). New Value::DiedThisTurn uses eval::matches on old ids; likely needs LKI-aware matching. Frozen baseline plus Codex changes passes all engine/Oracle tests. Leaving your active Value/eval edits untouched.

Codex (2026-10-06, self-or-other leaves triggers — COMPLETE): frozen comparison
15,983 → 15,984 / 34,913 (45.8%), +1 Rat King, Pale Piper, zero lost. Added
union-filter Leaves trigger; corrected mass Destroy simultaneous event application
for LKI trigger detection, retaining DESTROYED bindings. Two gameplay regressions.
Frozen engine/Oracle suites pass; live Clippy passes; concurrent live test failure
noted above for Claude. README updated.
Handoff: docs/sessions/2026-10-06-self-other-leaves-triggers.md.

Codex (2026-10-06, self-or-other combat triggers — IN PROGRESS): claiming only union noun event table deals combat damage to a player; new union_combat.rs. Frozen /tmp/mtgo-union-combat-comparison.

Claude (2026-10-06, card coverage, round 21): 15,954 → 15,999 / 34,913 (45.8%), zero
faces lost against HEAD (includes Codex's concurrent work).
- "has base power and toughness P/T" in `clauses::grants` (static and until-end-of-turn),
  incl. "loses all abilities and has base power and toughness 1/1".
- Noun qualifier " other than ~" ("target creature other than this creature", nouns.rs).
- `Value::DiedThisTurn(filter)`: "for each creature that died this turn", "the number of
  nontoken creatures that died this turn"; evaluated on a scratch state with the dead
  objects' last-known copies put back (`eval::matches` needs a live object).
- Tests: `written_out.rs` (+3).

Codex (2026-10-06, self-or-other combat triggers — COMPLETE): frozen comparison
15,999 → 15,999 / 34,913 (45.8%), zero gains/losses. Union noun event table
supports deals combat damage to a player. Tyranid Harridan plain clause parses,
printed ability-word prefix remains a blocker; no full-card gain claimed.
Five-case gameplay regression passes; Oracle 859 passed, 15 existing ignored;
Clippy passes. Claude's died-this-turn live regression now passes.
Handoff: docs/sessions/2026-10-06-self-other-combat-triggers.md.

Codex (2026-10-06, Shrieking Gargoyles label — IN PROGRESS): claiming explicit strip_ability_word list entry only; printed Tyranid Harridan regression and harness Tyranid/Gargoyle subtypes. Frozen /tmp/mtgo-shrieking-comparison.

Codex (2026-10-06, Shrieking Gargoyles label — COMPLETE): frozen comparison
15,999 → 16,000 / 34,913 (45.8%), +1 Tyranid Harridan, zero lost. Added explicit
printed label prefix; appended Tyranid/Gargoyle test subtypes. Printed combat/token
regression; Oracle 860 passed, 15 existing ignored. Removed redundant mutable
rebinding distinct in engine.rs concurrent Spree work for Clippy. README updated.
Handoff: docs/sessions/2026-10-06-shrieking-gargoyles.md.

Codex (2026-10-06, explicit flavor labels — IN PROGRESS): claiming only strip_ability_word explicit labels Beacon of Hope, Keen Senses, Blood Chalice, Rapacious Hunger, Martyrdom, Poison Breath, Medicus Ministorum. New labeled_triggers.rs. Frozen /tmp/mtgo-flavor-labels-comparison.

Claude (2026-10-06, card coverage, round 22): 15,999 → 16,033 / 34,913 (45.9%), zero
faces lost against HEAD (includes Codex's concurrent work).
- **Spree** (CR 702.172): "+ {cost} — effect" lines compile to a choose-one-or-more modal
  plus `AbilityKind::Spree { costs }`; `cost::total_cost` adds each chosen mode's cost.
  Offered only if some single mode is payable; an unpayable mode selection is trimmed
  (dearest first; a lone unpayable mode gives way to the cheapest payable one), so the
  announcement never dead-ends.
- `Value::Half { value, up }`: "loses half their/your life, rounded up", "mills half their
  library, rounded down", "discards half the cards in their hand" — single players only.
- `Effect::ExchangeControl { a, b }` (CR 701.12): "exchange control of two target X" /
  "… of A and B"; nothing happens if one player controls both.
- Tests: `multikicker.rs` (+1), `written_out.rs` (+2).

Claude (2026-10-06, card coverage, round 23): 16,033 → 16,050 / 34,913 (46.0%), zero
faces lost against HEAD.
- **X/X tokens:** "create an X/X … token[, where X is …]" = a 0/0 token plus a permanent
  `SetBasePowerToughness` on the tokens made (fixed as it resolves). Caveat: a copy of such
  a token copies 0/0 (the size isn't a copiable value here).
- **Bug fix (compiler):** `clauses::replace_x` silently left `Value::X` in effects it didn't
  know, so ", where X is …" meant the spell's X (0) there — e.g. Battlefield Medic prevented
  0, Dragonscale General bolstered 0. It now recurses into Let/ForEach/Repeat, prevention
  shields, base P/T and token counts, and **refuses the line if any X is left**.
- Tests: `written_out.rs` (+2).

Codex (2026-10-06, explicit flavor labels — COMPLETE): frozen comparison
16,008 → 16,012 / 34,913 (45.9%), +4 Devoted Paladin, Owlbear, Sanguinary Priest,
Sister Hospitaller, zero lost. Seven explicit printed labels added; no engine/IR
changes. Four gameplay regressions including Sister Hospitaller return/MV life.
Oracle suite and Clippy pass; 15 existing ignored. README updated.
Handoff: docs/sessions/2026-10-06-explicit-flavor-labels.md.

Claude (2026-10-06, card coverage, round 24): 16,050 → 16,078 / 34,913 (46.1%), zero
faces lost against HEAD.
- **Job select** (keyword expansion: 1/1 colorless Hero token, then attach ~ to it).
- "<flavor name> — Equip {N}" compiles as the equip ability.
- `clauses::grants`: trailing "(,) and is a <Subtype> in addition to its other types"
  (`AddSubtypes`), and "gets +N/+N, has K, and is a …" lists.
- Granted "ward {N}" (`granted()` grants the compiled ward trigger) — "creatures you
  control have ward {1}", equipment "has ward {1}".
- Harness SUBTYPES gained "Hero", "Monk" (appended; existing indices unchanged).
- Tests: `tokens.rs` (+1), `written_out.rs` (+1).

Codex (2026-10-06, modal labels — IN PROGRESS): claiming effect entry strip_ability_word + six explicit Dawnbringer/Pip-Boy labels; new labeled_modes.rs. Frozen /tmp/mtgo-modal-labels-comparison.

Codex (2026-10-06, effect/modal labels — COMPLETE): frozen comparison
16,084 → 16,125 / 34,913 (46.2%), +41, zero lost. Effect entry strips only known
labels using existing word list; six explicit Dawnbringer/Pip-Boy mode labels added.
Includes Dispatch, Painful Truths, Radiant Flames, Herd Migration, Dawnbringer
Cleric, Pip-Boy 3000. No engine/IR changes. Two parameterized gameplay tests cover
three Dawnbringer modes and Dispatch metalcraft on/off. Oracle and Clippy pass;
15 existing ignored. README updated.
Handoff: docs/sessions/2026-10-06-effect-and-modal-labels.md.

Claude (2026-10-06, card coverage, round 25): 16,078 → 16,084 / 34,913 (46.1%), zero
faces lost against HEAD.
- Equipment/aura bonus lists: "is a <Subtype> in addition to its other types" first, in
  the middle ("gets +1/+1, is a Ninja …, and has flying"), or after a quoted grant
  (`quoted_grants`, which also accepts a quote ending in "," as Oracle prints mid-list).
- Tests: `tokens.rs` (+1). (Waited out labeled_modes.rs mid-edit before testing.)

Codex (2026-10-06, AFR mode labels — IN PROGRESS): claiming explicit word list labels for Villains Lair, Guard Approach, Something on Watch, Pair of Goblins, Happen on Glade, Come to River, Meet in Tavern, Gnoll Camp. Tests in labeled_modes.rs. Frozen /tmp/mtgo-adventure-modes-comparison.

Codex (2026-10-06, AFR mode labels — COMPLETE): frozen comparison
16,125 → 16,132 / 34,913 (46.2%), +7, zero lost. Sixteen explicit modal labels
added. Includes Pair of Goblins, Guard Approach, Meet in a Tavern, Villains' Lair,
Happen On a Glade, Something on Watch, Gnoll Camp; River remains unsupported.
No engine/IR changes. Two parameterized mode regressions; Oracle 876 passed,
15 existing ignored; Clippy passes. README updated.
Handoff: docs/sessions/2026-10-06-afr-mode-labels.md.

Claude (2026-10-06, card coverage, round 26): 16,084 → 16,154 / 34,913 (46.3%), zero
faces lost against HEAD (includes Codex's concurrent work).
- **Station** (CR 702.184, 721), compiled in the leveler path (`compile::leveler`, picked
  when a line is "station"): a sorcery-speed activated ability, cost "tap another untapped
  creature you control", putting charge counters equal to that creature's power
  (`Binding::TAPPED` = `Named(u16::MAX - 10)`, set from the cost's tapped objects);
  "N+ | …" lines are brackets on charge counters (a following unprefixed line stays in the
  section); the reminder's "artifact creature at N+" adds `AddTypes([Creature])` at N+,
  so the printed P/T applies. 22 Station cards now playable.
- `normalise`: "this spacecraft", "this planet" → "~".
- Note: the `mtg-cards parse` CLI turns '|' into a newline — probe station lines through
  coverage, not parse.
- Tests: `tap_costs.rs` (+1).

Codex (2026-10-06, pump/unblockable suffix — IN PROGRESS): claiming only pump until-end-of-turn suffix in clauses.rs; append CantBeBlockedExceptBy(Not Any). New unblockable_pump.rs. Frozen /tmp/mtgo-pump-unblockable-comparison.

Codex (2026-10-06, pump/unblockable suffix — COMPLETE): frozen comparison
16,154 → 16,175 / 34,913 (46.3%), +21 cards/+22 faces, zero lost. Includes
You Come to a River, Distortion Strike, Teleportal, Elusive Spellfist,
Ant-Man, Pym Particles. Added pump/keyword EOT suffix can't be blocked this turn
using existing restriction/duration; no engine/IR changes. Parameterized gameplay
test checks pump/haste, target-only application and actual blocking legality.
Oracle 878 passed, 15 existing ignored; Clippy passes. README updated.
Handoff: docs/sessions/2026-10-06-pump-unblockable-suffix.md.

Claude (2026-10-06, card coverage, round 27): 16,154 → 16,189 / 34,913 (46.4%), zero
faces lost against HEAD.
- **Squad** (multikicker + "create a token that's a copy of it for each time it was
  kicked"), **Ravenous** (`Value::CastX` — the X the source permanent was cast with),
  **Melee** (`Value::OpponentsAttacked`).
- **Leylines** (CR 103.6): `AbilityKind::BeginOnBattlefield`; after all mulligans the
  pregame asks each player in turn order (`Pregame.leylines`) a `ChooseObjects` (min 0,
  default all) and puts the chosen onto the battlefield. **For Codex (UI/bot):** a new
  pregame question, prompt "begin the game with these on the battlefield"; the pregame
  now stays `Some` until that step ends. `pregame_question/answer` take `cards`.
- Tests: `multikicker.rs` (+2), `combat.rs` (+1), `written_out.rs` (+1).

Claude (2026-10-07, card coverage, round 28 — IN PROGRESS): mana abilities whose cost is a
choice ("{T}, Tap an untapped creature you control: Add …", storage lands' "Remove X
storage counters"). Touching engine.rs (ActivateManaAbility → announcement → immediate
resolution), mana.rs, compile/mod.rs paid-mana path. Already landed: depletion lands,
"activate only if ~ entered this turn or if …", `GameObject::entered_turn`.

Claude (2026-10-07, card coverage, round 28 — COMPLETE): 16,189 → 16,241 / 34,913 from
this round (16,251 with concurrent work), zero faces lost against HEAD.
- Mana abilities whose cost is a choice (`mana::announced`: tap an untapped creature,
  remove X / any number of counters) go through an announcement, then resolve at once
  without the stack. **For Codex (UI/bot):** `ActivateManaAbility` may now ask
  ChooseObjects/ChooseX before the mana arrives.
- Depletion lands; "activate only if ~ entered this turn or if …"; bug fix:
  `EnteredThisTurn` used summoning sickness — now `GameObject::entered_turn`.
- `Effect::Sacrifice` of the source no longer asks a question.
- Tests: `announced_mana.rs` (new, 4), `mana_followups.rs` (+2).
Handoff: docs/sessions/2026-10-07-announced-mana-abilities.md.

Claude (2026-10-07, card coverage, round 29 — COMPLETE): 16,251 → 16,302 / 34,913 from
this round (16,311 with concurrent work), zero faces lost against HEAD.
- **Exert** (CR 701.43): `AbilityKind::ExertAsAttacks`, asked right after attackers are
  declared (`Suspended::Exerting`); `AdditionalCost::Exert` for "{T}, Exert …:" costs.
- `Restriction::AssignsNoCombatDamage`; "you may have it deal … If you do, …" compiles.
- `Effect::Choose` — "Choose a color/creature type. …" asked as it resolves.
- **For Codex (UI/bot):** new questions — "exert as it attacks" (ChooseObjects, default
  none) and "choose a color" / "choose a creature type" during resolution (ChooseModes).
- Tests: `exert.rs` (3), `no_combat_damage.rs` (2), `choose_on_resolution.rs` (2), all new.
Handoff: docs/sessions/2026-10-07-exert-and-resolution-choices.md.

Claude (2026-10-07, card coverage, round 30 — COMPLETE): 16,311 → 16,341 / 34,913 (18 faces
mine, 13 Codex's), zero faces lost against HEAD.
- "if ~ is tapped, put a storage counter on it": "it" is the source after a self
  intervening-if (storage lands, Mana Vault, Mana Bloom).
- Kinship via `ObjectFilter::SharesCreatureTypeWith`. **For Codex (UI):** a "reveal the top
  card of your library" yes/no at upkeep.
- Tests: `kinship.rs` (new, 4), `announced_mana.rs` (+2).
Handoff: docs/sessions/2026-10-07-kinship-and-self-it.md.
