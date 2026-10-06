//! Effect primitives.
//!
//! An effect is a tree. The engine walks it depth-first, threading an
//! `EffectContext` that holds chosen targets, the announced `X`, and the
//! [`crate::selector::Binding`] slots. Order of evaluation is the order written,
//! which matches how printed text reads.
//!
//! The set is intentionally closed and small. A new card should almost always be
//! a new *arrangement* of these, not a new variant — and a proposed new variant
//! is a design review, because every variant added here must also be taught to
//! [`crate::footprint`] or it degrades trigger ordering for every card.

use mtg_core::{CardType, Color, CounterKind, Zone};
use serde::{Deserialize, Serialize};

use crate::{
    cost::Cost,
    selector::{Binding, ObjectFilter, Selector},
    trigger::Condition,
    value::Value,
};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Effect {
    /// Do nothing. Useful as an explicit "else" arm.
    Nothing,
    /// CR 615: prevent all combat damage until the cleanup step of this turn.
    PreventAllCombatDamage,
    /// "Damage can't be prevented this turn."
    DamageCantBePrevented,
    /// Prevent all damage to the selected recipients until cleanup.
    PreventDamage {
        to: Selector,
    },
    /// CR 615 — a prevention shield until cleanup: all damage, or the next `amount`, that
    /// would be dealt to each of `to` (anything, when `None`) by each of `by` (any source,
    /// when `None`). Who is protected is fixed as it resolves.
    PreventDamageShield {
        to: Option<Selector>,
        by: Option<Selector>,
        combat_only: bool,
        amount: Option<Value>,
    },
    /// Run each child in order.
    Sequence(Vec<Effect>),
    /// Run `then` if the condition holds, else `otherwise`.
    If {
        cond: Condition,
        then: Box<Effect>,
        otherwise: Box<Effect>,
    },
    /// The controller picks one (or more) of the listed modes on announcement
    /// (CR 700.2). Modes are chosen at cast time, not resolution.
    Modal {
        choose: Value,
        modes: Vec<(Box<str>, Effect)>,
        /// "Choose one or both", "choose one or more": the fewest modes, with `choose` the
        /// most. `None` means exactly `choose`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at_least: Option<u8>,
    },
    /// Bind the result of a selector to a slot, so later steps can say "it".
    Let {
        slot: Binding,
        what: Selector,
        body: Box<Effect>,
    },
    /// Repeat for each object a selector matches, binding `Binding::It`.
    ForEach {
        what: Selector,
        body: Box<Effect>,
    },
    /// Repeat a fixed number of times.
    Repeat {
        times: Value,
        body: Box<Effect>,
    },
    /// Give the controller the option; `Nothing` if declined.
    MayPay {
        cost: Cost,
        then: Box<Effect>,
    },
    May {
        prompt: Box<str>,
        then: Box<Effect>,
        /// What happens if the controller declines, or cannot: "sacrifice it unless you
        /// discard a card".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise: Option<Box<Effect>>,
    },
    /// "Sacrifice it unless you pay {2}{R}": the controller may pay; if they do not,
    /// `otherwise` happens. The mana is paid `times` times over ("{1} for each age
    /// counter on it"); paying it zero times costs nothing.
    UnlessPays {
        cost: Cost,
        #[serde(default = "crate::value::one")]
        times: Value,
        otherwise: Box<Effect>,
    },

    /// Add mana to a player's pool (CR 106).
    ///
    /// Mana production is its own primitive rather than a counter or a resource
    /// effect, because mana is unlike every other quantity in the game: it empties
    /// at the end of each step and phase (CR 500.4), it has colour, and an ability
    /// that produces it does not use the stack (CR 605).
    AddMana {
        who: Selector,
        produces: Vec<ManaOutput>,
    },

    // ---- life and cards -------------------------------------------------
    GainLife {
        who: Selector,
        amount: Value,
    },
    /// "You get {E}{E}" (CR 107.14).
    GainEnergy {
        who: Selector,
        amount: Value,
    },
    /// "Defending player gets a poison counter" (CR 122.1f).
    GivePoison {
        who: Selector,
        amount: Value,
    },
    LoseLife {
        who: Selector,
        amount: Value,
    },
    Draw {
        who: Selector,
        count: Value,
    },
    Discard {
        who: Selector,
        count: Value,
        at_random: bool,
    },
    /// Any move between zones: mill, bounce, exile, tutor, and reanimation are
    /// all this primitive with different endpoints.
    MoveZone {
        what: Selector,
        to: Zone,
        /// `None` keeps the object's current owner-relative zone.
        owner_relative_to: Option<Selector>,
        position: ZonePosition,
        tapped: bool,
        face_down: bool,
        /// Under whose control it arrives, if not its owner's.
        under_control_of: Option<Selector>,
    },
    /// Reveal selected cards to all players before subsequent effects.
    Reveal {
        what: Selector,
    },
    Shuffle {
        who: Selector,
    },
    /// "Look at the top N cards of your library, then put them back in any order."
    ReorderLibraryTop {
        who: Selector,
        count: Value,
    },
    /// "Look at the top four cards of your library. You may reveal a creature card from
    /// among them and put it into your hand. Put the rest on the bottom of your library in
    /// a random order." The controller looks at `count`, takes `take` matching `filter`
    /// (or up to that many) to `take_to`, and the rest go to `rest_to` — the bottom of
    /// the library, randomly or in an order they choose, or the graveyard.
    Dig {
        count: Value,
        take: Value,
        up_to: bool,
        filter: ObjectFilter,
        take_to: Zone,
        reveal: bool,
        rest_to: Zone,
        rest_random: bool,
    },
    /// Look at the top N and reorder/bin them: scry, surveil, and the "look at
    /// the top card" shapes.
    LookAndSort {
        who: Selector,
        count: Value,
        keep_zone: Zone,
        other_zone: Zone,
    },

    // ---- permanents -----------------------------------------------------
    /// CR 701.7. Not the same as `MoveZone`: destruction is replaceable and
    /// regeneration and indestructible apply to it.
    Destroy {
        what: Selector,
    },
    /// CR 701.16. A cost when it appears in a cost, an effect when it appears here.
    Sacrifice {
        who: Selector,
        what: Selector,
    },
    DealDamage {
        source: Selector,
        to: Selector,
        amount: Value,
    },
    /// CR 601.2d — "deals 3 damage divided as you choose among one, two, or three
    /// targets": one point of damage for each of `shares`, the target slots the division was
    /// announced with. A target named by several slots is dealt that many, all at once.
    DealDamageDivided {
        source: Selector,
        shares: Vec<Selector>,
    },
    Tap {
        what: Selector,
    },
    Untap {
        what: Selector,
    },
    AddCounters {
        what: Selector,
        kind: CounterKind,
        amount: Value,
    },
    RemoveCounters {
        what: Selector,
        kind: CounterKind,
        amount: Value,
    },
    Attach {
        what: Selector,
        to: Selector,
    },
    CreateToken {
        token: TokenSpec,
        count: Value,
        controller: Selector,
    },
    /// CR 610.3 — "exile target … until ~ leaves the battlefield": each selected permanent
    /// is exiled, and returns to the battlefield under its owner's control as soon as the
    /// source leaves. If the source has already left, nothing is exiled (CR 610.3c).
    ExileUntilSourceLeaves {
        what: Selector,
    },
    /// CR 702.35a — the madness trigger: its owner may cast the exiled card by paying
    /// `cost`; if they don't, it goes to their graveyard.
    Madness {
        cost: mtg_core::ManaCost,
    },
    /// Ninjutsu (CR 702.49a): put the card onto the battlefield tapped and attacking what
    /// the creature returned to pay the cost was attacking — bound in `like`. It was never
    /// declared as an attacker, so "whenever it attacks" does not trigger (CR 508.4).
    EnterAttacking {
        what: Selector,
        like: Binding,
    },
    /// CR 701.37a — the selected permanents become monstrous.
    BecomeMonstrous {
        what: Selector,
    },
    /// "You may play that card this turn": the controller may play the selected cards
    /// (exiled, typically) for as long as `until` says. `cast_only` is "you may cast".
    GrantPlay {
        what: Selector,
        until: PlayUntil,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        cast_only: bool,
    },
    /// "Its owner may cast this card after the current turn has ended for as long as it
    /// remains exiled" (warp): for its mana cost.
    GrantCastLater {
        what: Selector,
    },
    /// A reflexive trigger (CR 603.12): "you may pay {2}. When you do, …" — a triggered
    /// ability of its own, going on the stack (and choosing `targets`) once this resolves.
    Reflexive {
        effect: Box<Effect>,
        targets: Vec<crate::selector::TargetSpec>,
    },
    /// "Create a token that's tapped and attacking": the objects are attacking the defending
    /// player (CR 508.4) — if there is a combat.
    PutAttacking {
        what: Selector,
    },
    /// "Add {C}{C}. Spend this mana only to cast colorless spells." — mana that can pay
    /// only for spells matching `only`.
    SpendOnly {
        only: ObjectFilter,
        effect: Box<Effect>,
        /// "This mana can't be spent to cast a nonartifact spell" (a Powerstone): the
        /// restriction is on casting spells only, so the mana still pays for abilities
        /// and other costs.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        spells_only: bool,
    },
    /// "If that creature would die this turn, exile it instead."
    ExileIfDiesThisTurn {
        what: Selector,
    },
    /// "Take an extra turn after this one" (CR 500.7).
    ExtraTurn {
        who: Selector,
    },
    /// "You skip your next turn" (CR 614.10).
    SkipNextTurn {
        who: Selector,
    },
    /// CR 724.1 — a player becomes the monarch. The designation's two triggered abilities
    /// (CR 724.2) live on an object in the command zone, made from `emblem` the first time
    /// anyone becomes the monarch; the steal ability inside it carries `None`.
    BecomeMonarch {
        who: Selector,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emblem: Option<TokenSpec>,
    },
    /// CR 716.2a — the source Class becomes this level.
    GainClassLevel {
        level: u8,
    },
    /// CR 702.112a — the selected permanents become renowned.
    BecomeRenowned {
        what: Selector,
    },
    /// "Target opponent reveals their hand. You choose a nonland card from it. That player
    /// discards that card." The hand is revealed, the controller chooses one card matching
    /// `filter`, and it is discarded — or exiled, when `exile`.
    RevealHandChoose {
        who: Selector,
        filter: ObjectFilter,
        exile: bool,
    },
    /// CR 701.28 — transform each selected permanent that has a back face to turn to.
    Transform {
        what: Selector,
    },
    /// "Exile this Saga, then return it to the battlefield transformed under your
    /// control" (CR 712.14): it comes back with its back face up.
    ExileReturnTransformed {
        what: Selector,
    },
    /// CR 701.27 — proliferate: choose any number of permanents and/or players with
    /// counters; give each another counter of each kind already there.
    Proliferate,
    /// CR 702.85 — cascade: exile cards from the top of your library until a nonland card
    /// with lesser mana value than this spell; you may cast it without paying its mana
    /// cost; the rest go on the bottom in a random order.
    Cascade,
    /// CR 701.50 — each selected creature connives: its controller draws a card, then
    /// discards a card; if a nonland card was discarded, the creature gets a +1/+1 counter.
    Connive {
        what: Selector,
    },
    /// CR 701.44 — each selected creature explores: its controller reveals the top card
    /// of their library; a land goes to their hand, otherwise the creature gets a +1/+1
    /// counter and they may put the card into their graveyard.
    Explore {
        what: Selector,
    },
    /// "If it would leave the battlefield, exile it instead" (unearth, CR 702.84a): a
    /// replacement that stays with each selected permanent while it remains there.
    ExileIfLeaves {
        what: Selector,
    },
    /// CR 707.2, 111.10 — create tokens that are copies of an object: each has its
    /// copiable values, as it last existed if it has left the battlefield.
    CreateTokenCopy {
        of: Selector,
        count: Value,
        controller: Selector,
    },
    /// Gain control until the effect ends.
    GainControl {
        what: Selector,
        who: Selector,
        duration: Duration,
    },

    /// CR 603.7 — create a delayed triggered ability: once, the next time `on` happens,
    /// `effect` happens. What "it" means is captured when this resolves.
    Delayed {
        on: crate::trigger::EventPattern,
        effect: Box<Effect>,
    },

    /// CR 701.14 — two creatures fight: each deals damage equal to its power to the
    /// other. Nothing happens if either is gone or no longer a creature.
    Fight {
        a: Selector,
        b: Selector,
    },

    /// CR 701.15 — give each object a regeneration shield for this turn: the next time it
    /// would be destroyed, it is tapped, its damage removed and it leaves combat instead.
    Regenerate {
        what: Selector,
    },

    // ---- the stack ------------------------------------------------------
    CounterSpell {
        what: Selector,
        /// "If that spell is countered this way, exile it instead of putting it into its
        /// owner's graveyard."
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        exile: bool,
    },
    /// "Counter target spell unless its controller pays {3}", and ward (CR 702.21): the
    /// controller of each object may pay; any not paid for is countered.
    CounterUnlessPays {
        what: Selector,
        mana: mtg_core::ManaCost,
        /// Life to pay instead of mana ("ward—pay 3 life").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        life: Option<u32>,
        /// A card to discard instead of mana ("ward—discard a card").
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        discard: bool,
        /// A spell countered this way is exiled instead of going to the graveyard.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        exile: bool,
    },
    /// Copy a spell or ability on the stack, optionally letting new targets be chosen.
    CopySpell {
        what: Selector,
        may_change_targets: bool,
    },
    /// Cast or play without paying, the "free spell" shape. `haste`: a creature spell
    /// cast this way gains haste once it is a permanent (suspend, CR 702.62a).
    CastWithoutPaying {
        what: Selector,
        from: Zone,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        haste: bool,
    },

    // ---- continuous ------------------------------------------------------
    /// Create a continuous effect. This is how pumps, grants, type-changing and
    /// static abilities all enter the layer system; `layer` says where it applies.
    Continuous {
        what: Selector,
        modification: Modification,
        duration: Duration,
    },

    /// A card whose behaviour genuinely cannot be expressed above. Resolved by
    /// a Rust implementation looked up by this key. Opaque to footprint analysis,
    /// so it always prompts for ordering.
    Native {
        key: Box<str>,
    },
}

/// One mana an ability produces.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ManaOutput {
    Colored(Color),
    Colorless,
    /// The controller picks one of these when the ability resolves. This is what
    /// makes affordability a matching problem rather than a count: a source with
    /// options can satisfy any one of several requirements, but only one.
    AnyOf(Vec<Color>),
    /// One mana of the color its source's controller chose as it entered.
    ChosenColor,
    /// Several mana at once, all of the same kind.
    Repeated {
        amount: Value,
        output: Box<ManaOutput>,
    },
}

impl ManaOutput {
    /// The colours this output could produce. Empty means colorless.
    pub fn possible_colors(&self) -> Vec<Color> {
        match self {
            ManaOutput::Colored(c) => vec![*c],
            ManaOutput::Colorless => Vec::new(),
            ManaOutput::AnyOf(cs) => cs.clone(),
            // Known only from the source; mana sources resolve it first.
            ManaOutput::ChosenColor => Vec::new(),
            ManaOutput::Repeated { output, .. } => output.possible_colors(),
        }
    }

    /// Whether the controller has to choose what this produces.
    pub fn is_ambiguous(&self) -> bool {
        match self {
            ManaOutput::AnyOf(cs) => cs.len() > 1,
            ManaOutput::Repeated { output, .. } => output.is_ambiguous(),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ZonePosition {
    Top,
    Bottom,
    /// Nth from the top, for "second from the top" effects.
    FromTop(u8),
    /// Unordered zones and the battlefield.
    Natural,
    OwnerChooses,
}

/// What a continuous effect changes, tagged with its CR 613 layer. The layer is
/// part of the data because getting it wrong is the single most common source of
/// wrong answers in a rules engine, and stating it makes it reviewable.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Modification {
    /// Layer 1 — copy effects.
    CopyOf(Selector),
    /// Layer 2 — control-changing.
    Control(Selector),
    /// Layer 3 — text-changing.
    ChangeText {
        from: Box<str>,
        to: Box<str>,
    },
    /// Layer 4 — type-changing.
    AddTypes(Vec<CardType>),
    /// Layer 4 — "becomes a 2/2 Bear creature": subtypes added.
    AddSubtypes(Vec<mtg_core::Subtype>),
    /// Layer 4 — "except it isn't legendary".
    RemoveSupertype(mtg_core::Supertype),
    /// Layer 1 — a copy "with no mana cost" (embalm, eternalize).
    NoManaCost,
    RemoveTypes(Vec<CardType>),
    SetTypes(Vec<CardType>),
    /// Layer 5 — color-changing.
    AddColors(Vec<Color>),
    SetColors(Vec<Color>),
    /// Layer 6 — ability adding and removing.
    GrantAbility(Box<crate::ability::Ability>),
    LoseAllAbilities,
    /// "Loses flying": the keyword, printed or granted earlier in the layer.
    LoseKeyword(mtg_core::Keyword),
    /// Layer 7a — characteristic-defining power/toughness.
    /// Layer 7a/7b — power alone set: "~'s power is equal to the number of creatures you
    /// control" (a characteristic-defining ability; toughness is printed).
    SetBasePower(Value),
    SetBasePowerToughness {
        power: Value,
        toughness: Value,
    },
    /// Layer 7c — the ordinary `+N/+N` pump.
    ModifyPowerToughness {
        power: Value,
        toughness: Value,
    },
    /// Layer 7d — switch power and toughness.
    SwitchPowerToughness,
    /// Not a characteristic: a restriction or permission, which the engine
    /// consults when validating actions rather than when computing values.
    Restriction(Restriction),
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Restriction {
    CantAttack,
    /// "Can attack as though it didn't have defender" (CR 702.3b).
    AttackDespiteDefender,
    /// "Assigns combat damage equal to its toughness rather than its power" (CR 510.1a).
    AssignDamageByToughness,
    CantBlock,
    CantBeBlockedExceptBy(ObjectFilter),
    /// "Can block only creatures with flying": what this creature may block.
    CanBlockOnly(ObjectFilter),
    MustAttackIfAble,
    /// CR 701.38 — goaded: attacks each combat if able, and a player other than the one
    /// who goaded it if able. Matches are two-player, where the second half never binds.
    Goaded,
    CantBeTargetedBy(ObjectFilter),
    Indestructible,
    /// CR 701.5a cannot apply to this spell.
    CantBeCountered,
    CantUntapDuringUntapStep,
    /// CR 701.15c — regeneration shields do nothing for it.
    CantBeRegenerated,
    /// "You may cast creature spells from the top of your library", "you may play lands and
    /// cast spells from the top of your library": spells matching `spells`, and lands
    /// when `lands`.
    PlayFromTopOfLibrary {
        spells: ObjectFilter,
        lands: bool,
    },
    /// "Skip your draw step."
    SkipDrawStep,
    /// CR 702.171b — saddled (a Mount, until end of turn).
    Saddled,
    /// "You may look at the top card of your library any time."
    LookAtTopOfLibrary,
    /// "Play with the top card of your library revealed."
    TopOfLibraryRevealed,
    /// "Its activated abilities can't be activated" (Arrest): mana abilities included.
    CantActivateAbilities,
    /// "You have hexproof": the source's controller can't be the target of spells or
    /// abilities their opponents control (CR 702.11c).
    PlayerHexproof,
    /// "Players can't gain life", "your opponents can't gain life" — these players,
    /// relative to the source's controller.
    CantGainLife(crate::selector::Selector),
    /// CR 502.3 — "You may choose not to untap this creature during your untap step."
    MayChooseNotToUntap,
    /// CR 305.2a — "You may play an additional land on each of your turns": its
    /// controller may play one more land each turn.
    AdditionalLandPlay,
    /// "Can't be blocked by more than one creature."
    CantBeBlockedByMoreThanOne,
    /// "Can't be blocked except by three or more creatures": menace with a larger count
    /// (CR 509.1b), judged on the whole declaration.
    MinimumBlockers(u8),
    /// Granted protection (CR 702.16): "gains protection from red until end of turn".
    /// `chosen_color` means "from the color of your choice": the color is chosen as the
    /// effect begins and `from` becomes that color.
    Protection {
        from: ObjectFilter,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        chosen_color: bool,
    },
    /// "You may cast creature spells as though they had flash" (CR 702.8d): spells
    /// matching the filter that the source's controller casts. On a spell itself
    /// ("you may cast this spell as though it had flash if …"), `IsSelf`.
    FlashFor(ObjectFilter),
    /// "If damage would be dealt to this creature, prevent that damage. Remove a +1/+1
    /// counter from this creature." (the Phantoms): all of it prevented, one counter
    /// removed for damage dealt at once.
    PreventDamageRemoveCounter,
    /// "If you would gain life, you gain that much life plus 1 instead": these players
    /// (relative to the source's controller) gain `plus` more, and twice as much when
    /// `double` ("you gain twice that much life instead").
    LifeGainBoost {
        who: crate::selector::Selector,
        plus: i32,
        double: bool,
    },
    /// "Can't attack alone" (CR 506.5): legal only if another creature also attacks.
    CantAttackAlone,
    /// "Can't block alone" (CR 506.5): legal only if another creature also blocks.
    CantBlockAlone,
    /// "Can block an additional creature each combat" (CR 509.1a): this many more
    /// attackers; `None` is "can block any number of creatures". Several add up.
    BlockAdditional(Option<u8>),
    /// Block requirements (CR 509.1c). "Blocks each combat if able": it blocks some
    /// attacker if it can.
    MustBlock,
    /// "Target creature blocks this creature this turn if able": it blocks the effect's
    /// source if it can.
    MustBlockSource,
    /// "Must be blocked if able": at least one creature blocks it if any can.
    MustBeBlocked,
    /// "All creatures able to block this creature do so": every creature matching the
    /// filter (from this creature's point of view) that can block it, does.
    MustBeBlockedByAll(ObjectFilter),
    /// "Your opponents can't cast creature spells", "each player can't cast more than one
    /// spell each turn": the players `who` names (from the effect's controller) can't cast
    /// spells matching `spells` — at all, or once they have cast `beyond` spells this turn.
    CantCast {
        who: crate::selector::Selector,
        spells: ObjectFilter,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        beyond: Option<u8>,
    },
    /// "You may have this creature assign its combat damage as though it weren't
    /// blocked" (CR 510.1c): its controller may send all of it to what it attacks.
    AssignAsThoughUnblocked,
    /// "Can't attack unless defending player controls an Island".
    CantAttackUnlessDefenderControls(ObjectFilter),
    /// CR 402.2 — "You have no maximum hand size": the controller of the source.
    NoMaximumHandSize,
    /// CR 615 — a static prevention effect: damage that would be dealt to this object by
    /// a source matching `from` (`dealt_to`), and damage this object would deal
    /// (`dealt_by`), is prevented.
    PreventDamage {
        dealt_to: bool,
        dealt_by: bool,
        combat_only: bool,
        from: ObjectFilter,
    },
    /// Cost modification: reduce or increase what a matching spell costs.
    CostModifier {
        what: ObjectFilter,
        delta: Value,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Duration {
    /// Lasts as long as the source is on the battlefield — a static ability.
    WhileSourcePresent,
    UntilEndOfTurn,
    /// "This combat": ends as creatures are removed from combat (CR 511.3).
    UntilEndOfCombat,
    UntilYourNextTurn,
    /// One-shot effects that nevertheless create a lasting change with no end.
    Permanent,
    /// Until the source leaves the battlefield, for "exile until" shapes.
    UntilSourceLeaves,
    /// Through the affected permanent's controller's next untap step: "doesn't untap
    /// during its controller's next untap step".
    ThroughNextUntapStep,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TokenSpec {
    /// The card face this token is, once registered with a card table
    /// ([`crate::walk::register_tokens`]). Tokens are real faces so that everything
    /// reading printed characteristics — the layer system, the UI, a peer's copy of the
    /// match's card table — sees them without special cases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<mtg_core::CardId>,
    pub name: Box<str>,
    pub types: Vec<CardType>,
    pub subtypes: Vec<mtg_core::Subtype>,
    pub colors: Vec<Color>,
    pub power: Value,
    pub toughness: Value,
    pub abilities: Vec<crate::ability::Ability>,
}

/// What a permanent asks its controller to choose as it enters.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum EntryChoice {
    Color,
    CreatureType,
}

/// A replacement or prevention effect (CR 614, CR 615).
///
/// Modelled separately from `Effect` because replacements are *rewrites of a
/// pending event*, not actions: each applies at most once to a given event
/// (CR 614.5), and when several apply the affected player chooses the order.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Replacement {
    /// Which pending events this applies to.
    pub matches: crate::trigger::EventPattern,
    pub kind: ReplacementKind,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ReplacementKind {
    /// Drop the event entirely.
    Prevent,
    /// Reduce a numeric field (damage prevention shields).
    Reduce { amount: Value },
    /// Redirect damage to another recipient.
    Redirect { to: Selector },
    /// Send a zone change somewhere else — the "exile instead of graveyard" shape.
    RedirectZoneChange { to: Zone },
    /// Change how many of something happens ("draws twice that many").
    Multiply { factor: Value },
    /// Enters with counters or tapped.
    EntersModified { modification: Modification },
    /// CR 614.1c — "<this> enters tapped."
    EntersTapped,
    /// "This land enters tapped unless you control two or more other lands."
    EntersTappedUnless {
        condition: crate::trigger::Condition,
    },
    /// CR 707.9 — "You may have this creature enter as a copy of any creature on the
    /// battlefield": chosen as it resolves, among objects matching `of`.
    EntersAsCopy { of: ObjectFilter, optional: bool },
    /// CR 614.12 — "As this land enters, you may pay 2 life. If you don't, it enters
    /// tapped."
    EntersTappedUnlessPaysLife { amount: u32 },
    /// "As this enters, choose a color / a creature type" (CR 614.12): the choice is the
    /// permanent's own, read later by "the chosen color/type".
    EntersChoosing(EntryChoice),
    /// Unleash (CR 702.98a): "you may have this creature enter with a +1/+1 counter".
    EntersWithCounterIfChosen,
    /// Riot (CR 702.136a): it enters with your choice of a +1/+1 counter or haste.
    EntersWithCounterOrHaste,
    /// Devour N (CR 702.82a): as it enters, its controller may sacrifice any number of
    /// creatures; it enters with N +1/+1 counters for each.
    Devour(u8),
    /// CR 614.1c — "<this> enters with N <kind> counters on it."
    /// With `condition`, only if it holds as it enters (bloodthirst, CR 702.54).
    EntersWithCounters {
        kind: CounterKind,
        amount: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        condition: Option<crate::trigger::Condition>,
    },
    /// Replace the event with an arbitrary effect ("instead, ...").
    Instead { effect: Box<Effect> },
    /// Do the effect in addition to the event, without replacing it. Not strictly
    /// a replacement in the rules, but it shares the "rewrite the pending event"
    /// pipeline position.
    InAddition { effect: Box<Effect> },
}

/// How long a permission to play a card lasts.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PlayUntil {
    /// "this turn", "until end of turn".
    ThisTurn,
    /// "until the end of your next turn".
    EndOfYourNextTurn,
}
