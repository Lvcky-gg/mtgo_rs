//! The event vocabulary.
//!
//! Everything that happens in a game happens as an `Event`. Events are the only
//! thing triggered abilities observe (CR 603.2), the only thing replacement
//! effects rewrite (CR 614), and the only thing written to the replay log. The
//! engine never mutates state except by applying an event, which is what makes
//! replay, reconnect, and verification fall out for free.

use crate::{
    ids::{AbilityId, ObjectId, PlayerId, Timestamp},
    zone::ZoneRef,
};
use serde::{Deserialize, Serialize};

/// Sequence number in the game's event log. Monotonic, gap-free, and the unit of
/// acknowledgement for network resync.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct EventId(pub u64);

impl EventId {
    pub const FIRST: Self = Self(0);
    /// Bump and return the new value.
    pub fn advance(&mut self) -> Self {
        self.0 += 1;
        *self
    }
}

/// Why an event happened. Many triggers care about the cause rather than just
/// the shape ("sacrificed" vs. "destroyed" are both a battlefield→graveyard
/// zone change) and last-known-information lookups walk this chain.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Cause {
    /// A turn-structure step did it.
    TurnStructure,
    /// A state-based action did it (CR 704) — no controller, no stack.
    StateBasedAction,
    /// Resolution of the object on the stack with this id.
    Resolution(ObjectId),
    /// Payment of a cost, e.g. a sacrifice as an additional cost.
    CostPayment(ObjectId),
    /// A player took a special action or a turn-based action directly.
    PlayerAction(PlayerId),
    /// A combat damage assignment.
    Combat,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct StampedEvent {
    pub id: EventId,
    pub at: Timestamp,
    pub cause: Cause,
    pub event: Event,
}

/// Atomic changes to the game state.
///
/// Deliberately *small and literal*: an event says what changed, never why or
/// what should follow. Composite happenings ("destroy" = lethal-damage marking
/// plus a zone change, or a regeneration replacement) are expressed as several
/// events so that each one is individually replaceable and individually
/// observable.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Event {
    // ---- zones ----------------------------------------------------------
    /// CR 400.7. `new_object` is the identity the object takes on in `to`;
    /// `object` is dead after this event and only valid for last-known info.
    ZoneChange {
        object: ObjectId,
        new_object: ObjectId,
        from: ZoneRef,
        to: ZoneRef,
        /// Position within an ordered destination zone, from the top/bottom.
        index: Option<u32>,
    },
    Shuffled {
        player: PlayerId,
    },
    /// CR 111.1 — a token is created on the battlefield. It enters the battlefield, so
    /// "enters" triggers see it, but it comes from nowhere: there is no zone change.
    Created {
        object: ObjectId,
        card: crate::CardId,
        owner: PlayerId,
    },
    /// CR 701.15a — a regeneration shield replaced a destruction: the permanent is tapped,
    /// all damage is removed from it, and it is removed from combat.
    Regenerated {
        object: ObjectId,
    },
    /// A player sacrificed a permanent (CR 701.21): `object` is its identity on the
    /// battlefield, read through last-known information.
    Sacrificed {
        player: PlayerId,
        object: ObjectId,
    },
    /// A player scried (CR 701.22) or, with `surveil`, surveilled (CR 701.25). Not for
    /// "scry 0" (CR 701.22c).
    Scried {
        player: PlayerId,
        surveil: bool,
    },
    /// A player cycled a card (CR 702.29): `object` is the card in its new zone.
    Cycled {
        player: PlayerId,
        object: ObjectId,
    },
    /// All damage marked on a permanent is removed, outside the cleanup step (umbra
    /// armor, CR 702.89a).
    DamageRemoved {
        object: ObjectId,
    },
    /// A player gets poison counters other than from infect damage (toxic).
    Poisoned {
        player: PlayerId,
        amount: u32,
    },
    /// A regeneration shield was created on an object for this turn.
    ShieldGained {
        object: ObjectId,
    },
    /// CR 704.5e / 111.7 — a token that has left the battlefield ceases to exist. It is
    /// removed from the game entirely, not moved anywhere.
    CeasedToExist {
        object: ObjectId,
    },

    // ---- players --------------------------------------------------------
    LifeChanged {
        player: PlayerId,
        delta: i32,
    },
    Drew {
        player: PlayerId,
        object: ObjectId,
    },
    MillLike {
        player: PlayerId,
        count: u32,
    },
    /// CR 704.5b: a player tried to draw from an empty library. The attempt, not the
    /// empty library, is what loses the game, and it is only checked the next time
    /// state-based actions are checked — so it has to be recorded when it happens.
    AttemptedDrawFromEmptyLibrary {
        player: PlayerId,
    },
    Lost {
        player: PlayerId,
        reason: LossReason,
    },
    /// Mana added to a pool. `color` is `None` for colorless.
    ManaAdded {
        player: PlayerId,
        color: Option<crate::characteristics::Color>,
        amount: u16,
    },
    /// Mana spent from a pool paying a cost.
    ManaSpent {
        player: PlayerId,
        color: Option<crate::characteristics::Color>,
        amount: u16,
    },

    // ---- permanents -----------------------------------------------------
    TapChanged {
        object: ObjectId,
        tapped: bool,
    },
    /// Damage dealt to a permanent, recorded once even if it has several types.
    /// Its recipient kind determines the consequences (CR 120.3).
    DamageMarked {
        source: ObjectId,
        object: ObjectId,
        amount: u32,
        #[serde(default)]
        recipient: ObjectDamageKind,
        /// Whether the source had deathtouch when the damage was dealt. Recorded on
        /// the event because CR 704.5h asks what dealt the damage, not what the
        /// source looks like now — losing deathtouch afterwards does not save the
        /// creature.
        deathtouch: bool,
        /// CR 702.80/702.90 — wither or infect: dealt as -1/-1 counters, not marked.
        #[serde(default)]
        counters: bool,
    },
    DamageDealtToPlayer {
        source: ObjectId,
        player: PlayerId,
        amount: u32,
        /// CR 702.90b — infect: dealt as poison counters, not life loss.
        #[serde(default)]
        counters: bool,
    },
    CountersChanged {
        object: ObjectId,
        kind: CounterKind,
        delta: i32,
    },
    Attached {
        object: ObjectId,
        to: Option<ObjectId>,
    },
    PhasedOut {
        object: ObjectId,
        out: bool,
    },
    /// A resolving Adventure grants its controller permission to play this exile object.
    AdventureExiled {
        object: ObjectId,
        player: PlayerId,
    },
    /// The selected face or alternative characteristics as a card is played.
    FaceSelected {
        object: ObjectId,
        face: u8,
    },
    FaceDownChanged {
        object: ObjectId,
        face_down: bool,
    },

    // ---- the stack ------------------------------------------------------
    SpellCast {
        object: ObjectId,
        controller: PlayerId,
    },
    /// CR 707.10 — a copy of a spell is put on the stack (not cast). It has the original's
    /// announced choices, and `targets` when new ones were chosen for it.
    SpellCopied {
        original: ObjectId,
        copy: ObjectId,
        controller: PlayerId,
        targets: Option<Vec<Target>>,
    },
    AbilityPutOnStack {
        object: ObjectId,
        source: ObjectId,
        ability: AbilityId,
        controller: PlayerId,
    },
    /// A completed activation, after choices are made and costs can be paid.
    AbilityActivated {
        source: ObjectId,
        ability: AbilityId,
        loyalty: bool,
    },
    Targeted {
        object: ObjectId,
        target: Target,
    },
    Countered {
        object: ObjectId,
    },
    Resolved {
        object: ObjectId,
    },

    // ---- turn structure -------------------------------------------------
    StepBegan {
        turn: u32,
        active: PlayerId,
        step: Step,
    },
    StepEnded {
        turn: u32,
        active: PlayerId,
        step: Step,
    },
    PriorityReceived {
        player: PlayerId,
    },

    // ---- combat ---------------------------------------------------------
    Attacked {
        attacker: ObjectId,
        defender: Target,
        /// Captured before control changes or the attacked planeswalker leaves.
        #[serde(default)]
        defending_player: Option<PlayerId>,
    },
    Blocked {
        blocker: ObjectId,
        attacker: ObjectId,
    },
    BecameUnblocked {
        attacker: ObjectId,
    },

    // ---- continuous effects ---------------------------------------------
    /// A continuous effect began or ended applying. The layer system reads the
    /// set of live effects; this event exists so that the log fully determines
    /// state and so "until end of turn" cleanup is observable.
    ContinuousEffectBegan {
        effect: ObjectId,
        source: ObjectId,
    },
    ContinuousEffectEnded {
        effect: ObjectId,
    },
    /// Global combat-damage prevention begins on resolution and ends at cleanup.
    CombatDamagePreventionChanged {
        active: bool,
    },
    /// A recipient gains or loses prevention of all damage until cleanup.
    DamagePreventionChanged {
        target: Target,
        active: bool,
    },
    /// CR 508.4 — a creature is put onto the battlefield attacking: in combat, attacking
    /// `defender`, but it was never declared as an attacker.
    EnteredAttacking {
        attacker: ObjectId,
        defender: Target,
    },
    /// CR 726 — it becomes day (`true`) or night.
    DayNight {
        day: bool,
    },
    /// CR 701.28 — a transforming permanent turns over: `face` is now up.
    Transformed {
        object: ObjectId,
        face: u8,
    },
    /// A player gets or pays energy counters (CR 122.1c).
    EnergyChanged {
        player: PlayerId,
        delta: i32,
    },
    /// A permanent's controller chose a color or a creature type as it entered.
    ChoiceMade {
        object: ObjectId,
        color: Option<crate::Color>,
        subtype: Option<crate::Subtype>,
    },
    /// CR 701.37b — a permanent becomes monstrous.
    BecameMonstrous {
        object: ObjectId,
    },
    /// `player` may play this card until the end of turn `until_turn` ("you may play that
    /// card this turn"); only cast it, when `cast_only`.
    PlayPermission {
        object: ObjectId,
        player: PlayerId,
        until_turn: u32,
        cast_only: bool,
    },
    /// A foretold or plotted card: `player` may cast it from exile after turn `after_turn`,
    /// for `cost` (`None`: without paying its mana cost), only as a sorcery when `sorcery`.
    CastLater {
        object: ObjectId,
        player: PlayerId,
        after_turn: u32,
        cost: Option<crate::ManaCost>,
        sorcery: bool,
    },
    /// A permanent put onto the battlefield "under your control" (CR 110.2a): its
    /// controller from the start.
    EnteredUnderControl {
        object: ObjectId,
        player: PlayerId,
    },
    /// The mana spent to cast a spell, by color (`None` colorless) — what sunburst,
    /// converge and "if {R} was spent to cast it" read.
    SpentToCast {
        object: ObjectId,
        mana: Vec<(Option<crate::characteristics::Color>, u16)>,
    },
    /// CR 702.179 — a player's speed is now this.
    SpeedChanged {
        player: PlayerId,
        speed: u8,
    },
    /// An Aura becomes attached to a player (CR 303.4f, "enchant player").
    AttachedToPlayer {
        object: ObjectId,
        player: PlayerId,
    },
    /// "If it would die this turn, exile it instead": through turn `turn`.
    ExileIfDies {
        object: ObjectId,
        turn: u32,
    },
    /// A reflexive trigger fires (CR 603.12): the delayed trigger with this id.
    ReflexiveTriggered {
        id: u32,
    },
    /// CR 500.7 — `player` will take an extra turn after this one.
    ExtraTurnAdded {
        player: PlayerId,
    },
    /// CR 724 — a player becomes the monarch. `emblem` is the object that carries the
    /// designation's triggered abilities in the command zone, created the first time.
    BecameMonarch {
        player: PlayerId,
        emblem: ObjectId,
        card: crate::CardId,
    },
    /// CR 716.2 — a Class gains a level.
    ClassLevelGained {
        object: ObjectId,
        level: u8,
    },
    /// CR 702.112b — a permanent becomes renowned.
    BecameRenowned {
        object: ObjectId,
    },
    /// A permanent entered with "you control enchanted …": its controller controls what it
    /// is attached to.
    ControlsHost {
        object: ObjectId,
    },
    /// CR 702.35a — a card with madness was discarded, and exiled instead of being put
    /// into its owner's graveyard. It still counts as discarded.
    MadnessExiled {
        object: ObjectId,
        player: PlayerId,
    },
    /// The bottom `count` cards of a player's library are put in a random order
    /// ("put them on the bottom of your library in a random order").
    LibraryBottomShuffled {
        player: PlayerId,
        count: u32,
    },
    /// A card is revealed to every player (CR 701.20).
    Revealed {
        object: ObjectId,
    },
    /// A permanent gains "if it would leave the battlefield, exile it instead" (unearth).
    ExileIfLeaves {
        object: ObjectId,
    },
    /// CR 707.2 — a permanent becomes a copy of another object: its copiable values are
    /// now those of `card`'s face `face`, until it leaves the battlefield.
    BecameCopy {
        object: ObjectId,
        card: crate::CardId,
        face: u8,
    },
    /// A prevention shield (CR 615) begins; it lasts until cleanup.
    DamageShieldCreated {
        shield: DamageShield,
    },
    /// A counting shield prevented `amount` damage; one with nothing left ends.
    DamageShieldUsed {
        shield: u32,
        amount: u32,
    },
    DamageShieldEnded {
        shield: u32,
    },
}

/// A prevention effect created by a resolving spell or ability (CR 615): all damage, or
/// the next `remaining` damage, that would be dealt to `to` by `by` this turn.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DamageShield {
    pub id: u32,
    /// The recipient it protects; `None` for any.
    pub to: Option<Target>,
    /// The source whose damage it prevents; `None` for any.
    pub by: Option<ObjectId>,
    /// Only combat damage (CR 510).
    pub combat_only: bool,
    /// How much more it prevents; `None` for all of it (CR 615.7).
    pub remaining: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum LossReason {
    ZeroOrLessLife,
    DrawFromEmptyLibrary,
    PoisonCounters,
    /// CR 903.10a — 21 or more combat damage from a single commander.
    CommanderDamage,
    StateBasedEffect,
    Concede,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum CounterKind {
    PlusOnePlusOne,
    MinusOneMinusOne,
    Loyalty,
    Defense,
    Poison,
    /// CR 122.1d — a permanent with a stun counter that would become untapped instead
    /// loses a stun counter.
    Stun,
    /// CR 122.1c — a permanent with a shield counter that would be dealt damage or
    /// destroyed instead loses a shield counter.
    Shield,
    Other(u16),
}

/// Anything that can be targeted or be a combat defender.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Target {
    Object(ObjectId),
    Player(PlayerId),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Step {
    Untap,
    Upkeep,
    Draw,
    PrecombatMain,
    BeginCombat,
    DeclareAttackers,
    DeclareBlockers,
    CombatDamage,
    FirstStrikeCombatDamage,
    EndCombat,
    PostcombatMain,
    End,
    Cleanup,
}

impl Step {
    /// CR 117.1a: no player receives priority during untap or (normally) cleanup.
    pub const fn grants_priority(self) -> bool {
        !matches!(self, Step::Untap | Step::Cleanup)
    }

    pub const fn is_main_phase(self) -> bool {
        matches!(self, Step::PrecombatMain | Step::PostcombatMain)
    }

    /// The steps of the combat phase in which creatures are attacking (CR 506).
    pub const fn is_combat(self) -> bool {
        matches!(
            self,
            Step::DeclareAttackers
                | Step::DeclareBlockers
                | Step::FirstStrikeCombatDamage
                | Step::CombatDamage
                | Step::EndCombat
        )
    }
}

/// The types relevant to damage, captured before simultaneous damage is applied.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum ObjectDamageKind {
    #[default]
    Creature,
    Planeswalker,
    CreaturePlaneswalker,
}
