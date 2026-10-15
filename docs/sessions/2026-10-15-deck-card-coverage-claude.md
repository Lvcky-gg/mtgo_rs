# Deck coverage alongside Codex (Claude session) — 2026-10-15

Worked in parallel with the Codex deck-priority session, on cards it had not claimed.
On `/tmp/card-coverage.sqlite`, the Thrasios/Vial Smasher list is now 100 / 100 and the
Araumi list 92 / 100. Whole-catalog coverage is 18,022 / 34,954, a net count that includes
Codex's concurrent work. Changes remain uncommitted.

## Cards

- **Veil of Summer**: per-turn spell colors by player (`spell_colors_by_player`,
  `Condition::CastColorThisTurn`). "Spells you control can't be countered this turn" is a
  dynamic stack-zone effect, so it covers later spells. Hexproof from colors for players and
  permanents (`PlayerHexproofFrom`, `HexproofFrom`).
- **Urza's Saga**: exact printed-cost filter `ObjectFilter::ManaCostIs` ({0} is not "no
  cost"). Nested single quotes inside a granted ability's quote are normalised.
- **Gemstone Caverns**: `BeginOnBattlefieldNotStarting` (only for a player who isn't the
  starting player; adds the luck counter, then asks which card to exile).
  `ManaOutput::Conditional` handles "Add {C}. If …, instead add …" and "… add {B} instead".
- **Demonic Consultation**: `Effect::ExileUntilNamed`. The name choice offers the caster's
  own card names plus "a card not in your deck", which covers every distinct outcome
  without revealing the opponent's deck.
- **Gifts Ungiven / Intuition**: `Effect::SearchAndSplit`. Different names are found one at
  a time; the opponent picks min(n, found).
- **Necropotence**: face-down exile is now supported for `MoveZone` to exile; "your next end
  step" is a delayed timing; "exile that card from your graveyard" acts only while the card
  is still there.
- **Underworld Breach / Yawgmoth's Will**: `Restriction::GrantEscape`. Graveyard casting
  goes through `cost::graveyard_cast_now` / `graveyard_additional_now`, which include
  granted permissions.
- **Snapcaster Mage / Vohar**: `Restriction::GraveyardCast { exile }` on the target.
- **Yawgmoth's Will**: its exile replacement takes effect immediately, so the Will exiles
  itself.
- **Vivi Ornitier**: `ManaOutput::Combination`. The planner records a per-unit color split
  (`Payment::splits`), so one activation pays {U}{R}. Activating by hand makes a single
  color. "Activate only during your turn and only once each turn" works on mana abilities,
  and a {0} cost counts as free for mana sources.
- **Dauthi Voidwalker / Leyline of the Void**: `Restriction::GraveyardToExile` and a
  `graveyard_exiled` map. The void counter is added after the redirected move. Play
  permissions can now be free (`may_play_free`).
- **Araumi**: `Effect::GrantEncore` uses the card's own mana cost. The encore builder now
  lives in `mtg_ir::ability::encore`. Costs can exile a computed number of cards, and
  granted abilities on graveyard cards are offered.

## Compiler fix

"If X, A and B" now makes both A and B conditional; before, B always happened.

## Verification

- New compiled tests: `veil_of_summer`, `urzas_saga`, `gemstone_caverns`,
  `demonic_consultation`, `gifts_ungiven`, `necropotence`, `underworld_breach`,
  `vivi_ornitier`, `dauthi_voidwalker`, `graveyard_casting`, `araumi`.
- `cargo test --workspace` passes, and `cargo clippy --workspace --all-targets -D warnings`
  is clean.
- One regression fixture (`issue_local_mana_generic_phyrexian_competition`) was re-recorded.
  Its only differences are the new `spell_colors_by_player` field and the digests.

## Round 2: the rest of the Araumi list (now 100 / 100)

- **Ashiok, Dream Render / Opposition Agent**: `resolve::search_rule` decides each
  library search. Ashiok forbids a search its opponent causes themselves. Under Opposition
  Agent, the Agent's controller makes the choices and the found cards are exiled for them
  to play, with mana of any type (`may_play_any_mana`, `cost::any_mana`). The hook covers
  tutor/fetch `MoveZone`s, Gifts/Intuition and Beseech; library-and-graveyard searches
  honor Ashiok only.
- **Gilded Drake**: `Effect::ExchangeOrSacrifice`. Its trigger is exempt from fizzling
  ("still resolves if its target becomes illegal").
- **Mirror-Mad Phantasm**: `Effect::ShuffleInAndRevealSelfName`.
- **Mnemonic Betrayal**: exiles opponents' graveyards, grants a cast-only permission with
  any-type mana this turn, and returns what's left at the end step.
- **Beseech the Mirror**: `Effect::SearchExileMayCast` (bargain is the existing kicker).
- **Animate Dead / Necromancy**: `Effect::ReanimateAttach` and `SacrificeReanimated`,
  linked through `GameState::reanimated`. "Enchant creature card in a graveyard" is legal
  in the graveyard, and the link keeps the Aura legal on the returned creature.

Deck priorities: 202 / 209. Catalog: 18,030 / 34,954. Workspace tests and clippy pass.

## Round 3: Blue Farm (Kraum + Tymna, EDHREC cEDH average list) — 99 / 99

- **Starting Town**: `Condition::YourEarlyTurn` and `GameState::turns_taken`, counted as each
  untap step begins.
- **Tymna the Weaver**: `Value::CombatDamagedThisTurn` and `combat_damaged_this_turn`
  (damage with `Cause::Combat`). `MayPay` accepts a computed life amount.
- **Birgi**: `Effect::PersistMana` and `persistent_mana`, kept through step ends and gone
  after cleanup. Boast now has its own `ActivationTiming::Boast`, which
  `cost::activation_limit` raises to twice on your turn under `Restriction::BoastTwice`.
- **Mockingbird**: `Value::ManaSpent`, and copy exceptions on `EntersAsCopy { except }`.
  The exceptions are applied as permanent effects on the copy rather than as layer-1
  copiable values.
- **Chain of Vapor**: `Effect::BounceChain`. The copy is made under the sacrificing
  player's control, and they choose its new target.
- **Into the Flood Maw**: "Gift a tapped Fish" plus its "if the gift was promised,
  instead …" line compile together as two modes, each with its own target. A gift line
  without that partner line stays unsupported.

Two replay fixtures were re-recorded; their only differences are the new state fields and
the digests. The creature-type helpers in two older tests scanned only subtypes 0–63; the
harness now has more, so they scan wider. Catalog: 18,046 / 34,954.
