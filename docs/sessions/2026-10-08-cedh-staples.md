# Card coverage: cEDH staples first

The user asked to prioritize cEDH staples, then EDH staples. `scripts/fetch_staples.py`
fetches ranked lists (EDHTop16 tournament play rate for cEDH; EDHREC top cards for EDH)
and the new `mtg-cards check <file>` reports which listed cards are playable and the
lines blocking the rest (an exact card name wins over a face name: "Demonic Tutor", not
the back face of a later card).

| List (fetched 2026-10-08) | Before | After |
|---|---|---|
| cEDH top 100 | 53 | 69 |
| cEDH top 200 | 108 | 125 |
| cEDH top 400 | 210 | 230 |
| all cEDH staples (1,336) | 690 | 718 |
| EDH staples (2,571) | 1,211 | 1,244 |

Overall coverage 17,204 → 17,283 / 34,913 (49.5%); zero faces lost against a HEAD build.

## Shapes taught (each with end-to-end tests in `mtg-oracle/tests/compiled/`)

- **Another player pays** — `Effect::UnlessPays` gained `payer`; "you may draw a card
  unless that player pays {1}" (Rhystic Study, Mystic Remora), "{X}, where X is this
  creature's power" (Esper Sentinel), "that player may pay {2}. If the player doesn't, …"
  (Smothering Tithe). The opponent is asked first and pays from their own mana; one who
  can't pay isn't asked. `unless_player_pays.rs`.
- **Nth spell triggers** — `NthSpellCast { n, by, noncreature }`: "a player casts their
  second spell each turn" (Lotho), "an opponent casts their first noncreature spell each
  turn". New `GameState::noncreature_spells_by_player`, counted at cast time; omitted from
  verification snapshots while empty. A `?` in the general cast-trigger branch stopped
  "a player casts their …" from reaching this code; it now falls through.
- **Conditional free casts** — "If you control a commander, you may cast this spell
  without paying its mana cost" (`ObjectFilter::IsCommander`), "If an opponent cast three
  or more spells this turn, you may pay {0} …" (`Value::MostSpellsCastThisTurn` — one
  opponent's count, not the sum). `conditional_free_casts.rs`.
- **Change targets** — `Effect::ChangeTargets` / `Event::TargetsChanged`: "You may choose
  new targets for target spell or ability" (Deflecting Swat), legality judged for the
  retargeted spell; noun "spell or ability". `change_targets.rs`.
- **Spirit guides** — "Exile this card from your hand: Add {R}." is a mana source in hand
  (`mana::mana_sources`), exiled as its cost. `spirit_guides.rs`.
- **Pacts and game results** — `Effect::LoseGame` / `WinGame`, `LossReason::Effect`; "At
  the beginning of your next upkeep, pay {…}. If you don't, you lose the game."; a player
  who loses to an effect leaves the game at the next settle (CR 800.4a). `pacts.rs`,
  `mtg-verify/tests/leave_game.rs`.
- **Statics** — "Spells you control can't be countered" (Hexing Squelcher); Grand
  Abolisher's "your opponents can't cast spells or activate abilities of …" during your
  turn (mana abilities included). `uncounterable.rs`, `abolisher.rs`.
- **Rituals** — "… instead if <condition>" as a suffix and as its own line (Cabal Ritual's
  threshold), "add {R} for each …" in spells, "card named ~ in each graveyard" (Rite of
  Flame). `rituals.rs`.
- **One-shot flash** — "You may cast spells this turn as though they had flash" (Borne
  Upon a Wind); flash permissions now use a resolved effect's recorded controller.

One verification fixture (`issue_local_malformed_target_rejection.json`) was refreshed: its
only difference is the new noncreature-spell counter at the last checkpoint.

## Left in the cEDH top 100

Boseiju/Otawara (channel targets and cost reduction), Ragavan, Tainted Pact, Deathrite
Shaman, Underworld Breach, Delighted Halfling, Orcish Bowmasters, Veil of Summer,
Wishclaw Talisman, Demonic Consultation, Finale of Devastation, Chain of Vapor, Nature's
Rhythm (harmonize), Tataru Taru, Necropotence, Jeska's Will, Mockingbird, Pyroblast,
Sevinne's Reclamation, Thassa's Oracle, Beseech the Mirror, Ad Nauseam, Final Fortune
(needs the extra turn itself tracked), Into the Flood Maw (gift), Culling Ritual, Tinder
Wall, Mnemonic Betrayal, Shifting Woodland, Derevi, Birgi.

## Round 2

| List | After round 1 | After round 2 |
|---|---|---|
| cEDH top 100 | 69 | 70 |
| cEDH top 200 | 125 | 137 |
| cEDH top 400 | 230 | 242 |
| all cEDH staples | 718 | 736 |
| EDH staples | 1,244 | 1,262 |

Overall 17,327 / 34,913 (49.6%); zero faces lost against HEAD.

- **Command Tower, Arcane Signet** — `ManaOutput::CommanderIdentity`, from
  `mana::commander_identity` (mana cost, rules-text symbols, color indicator; none with no
  or a colorless commander). `command_tower.rs`.
- **Misdirection, Redirect Lightning** — `ChangeTargets.must` (CR 115.7a: the current
  target isn't offered when another is legal), `ObjectFilter::SingleTarget`.
- **Orim's Chant** — "target player can't cast spells this turn": `Selector::Player`,
  frozen by the resolver into the can't-cast effect. `orims_chant.rs`.
- **Cyclonic Rift** — "return each … to its owner's hand" (the overloaded reading).
- **Ashnod's/Phyrexian Altar, Phyrexian Tower** — a chosen sacrifice in a mana ability's
  cost (announced, manual). **Lion's Eye Diamond** — `AdditionalCost::DiscardHand`; an
  announced mana ability is manual-only, which is "activate only as an instant".
  `announced_mana.rs`.
- **Drannith Magistrate** — `ObjectFilter::InZone`: "can't cast spells from anywhere
  other than their hands", commanders included. `drannith.rs`.
- **Seedborn Muse** — `Restriction::UntapDuringOthersUntap`. `seedborn.rs`.
- **Teferi, Time Raveler** — `Restriction::CastOnlyAsSorcery`, checked in
  `cost::timing_allows` and the alternative-cost offer; "until your next turn, you may cast
  sorcery spells as though they had flash". `teferi_raveler.rs`.
- **Thassa's Oracle** — a fallback value comparison condition ("<value> is greater than or
  equal to <value>"), "the number of cards in your library", and `Cx::defined_x` so the
  dig's "where X is …" still defines a later "X". `thassas_oracle.rs`.

## Round 3

| List | After round 2 | After round 3 |
|---|---|---|
| cEDH top 100 | 70 | 71 |
| cEDH top 200 | 137 | 140 |
| cEDH top 400 | 242 | 251 |
| all cEDH staples | 736 | 747 |
| EDH staples | 1,262 | 1,283 |

Overall 17,363 / 34,913 (49.7%); zero faces lost against HEAD.

- **Chrome Mox** — imprint "exile a <card> from your hand" as `ExileLinked` over a chosen
  card; `ManaOutput::ExiledCardColors`. **Mox Amber** — `ManaOutput::ColorsAmong`.
  **Fellwar Stone / Exotic Orchard** — `ManaOutput::LandColors` (CR 106.7; lands asking
  the same question are skipped). `mana::concrete_output` replaces `with_chosen_color`
  and makes every source-dependent output concrete. `moxen.rs`, `fellwar.rs`.
- **Thriving lands** — `ReplacementKind::EntersChoosingColorExcept`, `ManaOutput::
  OrChosen`. `thriving_lands.rs`.
- **Toxic Deluge** — "pay X life" additional cost: X announced up to the caster's life,
  paid as life. `toxic_deluge.rs`.
- **Deafening Silence** — one noncreature spell each turn. **Grove of the Burnwillows** —
  opponents' life in a mana ability's follow-up. **City of Traitors** — `EventPattern::
  PlaysLand` (detection now sees each event's cause: a land put onto the battlefield by
  its player's action was played). "If <condition>, this land enters tapped."
- **Choose one; both if …** — `Value::If`, read as the spell is cast (Jeska's Will, Soul
  Transfer; most other Wills fail on a bullet). "you control an artifact and an
  enchantment"; "for each card in target opponent's hand" — a spell's mana amount is now
  counted with its targets. `choose_both.rs`.

Deferred: Mox Diamond needs a hook before a permanent enters (its "instead" sends it to
the graveyard without entering); storage lands' "any combination" needs per-mana color
choice.

## Round 4

| List | After round 3 | After round 4 |
|---|---|---|
| cEDH top 100 | 71 | 71 |
| cEDH top 200 | 140 | 145 |
| cEDH top 400 | 251 | 257 |
| all cEDH staples | 747 | 757 |
| EDH staples | 1,283 | 1,288 |

Overall 17,395 / 34,913 (49.8%); zero faces lost against HEAD.

- **Mana planner bug (engine):** a permanent with two tapping mana abilities (a basic
  land type's intrinsic ability plus a printed one, a painland, any land with two
  `{T}: Add …` lines) could pay with both. `mana::build_units` now gives such a permanent
  one unit (the union of colors when each ability makes one mana, else only its most
  productive ability), and `charge` activates the ability that makes the color spent
  (a painland paying {R} still deals its damage). Tests in `extra_mana.rs`.
- **Thrasios** — "reveal the top card of your library. If it's a <card>, …. Otherwise,
  …" (`reveal_top_branch`). `thrasios.rs`.
- **Bloom Tender / Faeburrow Elder** — `ManaOutput::EachColorAmong`, expanded by
  `mana::concrete_outputs`; "vivid" ability word. `bloom_tender.rs`.
- **Wild Growth, Utopia Sprawl, Badgermole Cub, Mana Flare–style** — `Restriction::
  AddsAdditionalMana` (CR 605.1b): the planner counts the extra mana and tapping for mana
  adds it. "enchanted Forest" nouns. `extra_mana.rs`.
- **Archivist of Oghma** — `Event::LibrarySearched` (emitted when `MoveZone` resolves a
  choice from a library, found or not) and `EventPattern::SearchesLibrary`. `archivist.rs`.
- **Endurance (and the MH2 evoke elementals)** — "Evoke—Exile a <color> card from your
  hand" (the engine now offers and pays evoke's non-mana parts), "up to one target
  player", `ZonePosition::BottomRandom`. `endurance.rs`.

## Round 5

| List | After round 4 | After round 5 |
|---|---|---|
| cEDH top 100 | 71 | 72 |
| cEDH top 200 | 145 | 148 |
| cEDH top 400 | 257 | 260 |
| all cEDH staples | 757 | 763 |
| EDH staples (all) | 1,288 | 1,296 |
| EDHREC top 100 | 83 | 88 |

Overall 17,406 / 34,913 (49.9%); zero faces lost against HEAD.

Engine fixes found by these cards:

- **A spell's controller is its caster** (CR 601.2a). Zone changes never set a controller,
  so a card cast from another player's zone (Ragavan's stolen card) stayed under its
  owner — "you gain 3 life" gave its owner the life. `apply::settle_controller`: a card
  put onto the stack by a player's action is theirs; off the battlefield and stack, a card
  is its owner's (CR 108.4a). Dies triggers now take their controller from last-known
  information (CR 603.3a) rather than the graveyard object. Two Commander fixtures were
  refreshed: the only change is a dead commander's controller now being its owner.
- **Modal spells are castable when enough modes are** (CR 700.2b): `targeting::
  can_be_announced` required every mode's targets; Rakdos Charm wasn't castable without
  an artifact to target.

Cards and shapes:

- **Mox Diamond** — `ReplacementKind::EntersIfDiscards`: asked before the spell enters;
  without a discard it goes to the graveyard without entering. (Put onto the battlefield
  by an effect, it isn't asked.) `mox_diamond.rs`.
- **Ragavan** — "exile the top card of that player's library". `ragavan.rs`.
- **Propaganda, Ghostly Prison** — `Restriction::AttackTax`, paid as attackers are
  declared; unaffordable declarations are refused. `propaganda.rs`.
- **Faeburrow Elder** ("for each color among …"), **Damn** ("a creature destroyed this way
  can't be regenerated", overloaded too), **Rakdos Charm** ("each creature deals 1 damage
  to its controller"). `edh_staples.rs`.
