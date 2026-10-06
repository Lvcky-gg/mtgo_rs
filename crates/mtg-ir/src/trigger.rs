//! Triggered abilities (CR 603).
//!
//! A trigger is a *pattern over events* plus the zone its source must be in.
//! This is the piece that makes trigger handling legible: when a trigger fires,
//! the engine can hand the UI the pattern and the event that matched it, which
//! is a complete and human-readable answer to "why is this on the stack?".

use mtg_core::{CounterKind, Step, Zone};
use serde::{Deserialize, Serialize};

use crate::{
    selector::{ObjectFilter, Selector},
    value::Value,
};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Trigger {
    pub on: EventPattern,
    /// Where the source must be for the ability to function (CR 603.6). Almost
    /// always `Battlefield`; graveyard and hand triggers set it explicitly.
    pub functions_from: Zone,
    /// An "intervening if" clause (CR 603.4): checked when the trigger would go
    /// on the stack *and again* on resolution, and it is not a cost.
    pub intervening_if: Option<Condition>,
    /// CR 603.2b — "may" triggers still use the stack; the choice happens on
    /// resolution.
    pub optional: bool,
    /// Caps like "only once each turn".
    pub limit: Option<TriggerLimit>,
    pub timing: TriggerTiming,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TriggerTiming {
    /// The ordinary case: goes on the stack the next time a player would receive
    /// priority (CR 603.3).
    Normal,
    /// CR 603.10: the event is a zone change away from the source's zone, so the
    /// trigger condition is checked against the game state *before* the event,
    /// using last-known information.
    LeavesBattlefield,
    /// CR 603.8: a state trigger. Fires whenever its condition is true and it is
    /// not already on the stack, rather than on a discrete event.
    StateTrigger,
    /// CR 603.6c/d: a replacement-like "as this enters" that never uses the
    /// stack at all.
    AsEnters,
    /// CR 603.11: triggers only while the source is on the stack, i.e. a spell's
    /// own cast trigger.
    WhileOnStack,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TriggerLimit {
    OncePerTurn,
    OncePerTurnPerSource,
    /// "The first time each turn" — distinct from once-per-turn in that later
    /// occurrences do not trigger at all rather than being capped.
    FirstEachTurn,
    /// "Whenever one or more creatures die": once for each batch of simultaneous events
    /// (CR 603.2c) — one combat damage step, one set of state-based actions, or one
    /// resolving spell or ability.
    OncePerBatch,
}

/// Matches against [`mtg_core::Event`].
///
/// Patterns are structural rather than textual, so the engine never parses
/// English at runtime. Mapping printed text onto a pattern happens once, at
/// import time, in `mtg-oracle`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum EventPattern {
    /// "Whenever <filter> enters" — a zone change ending on the battlefield.
    Enters {
        who: ObjectFilter,
    },
    /// "Whenever <filter> leaves the battlefield".
    Leaves {
        who: ObjectFilter,
    },
    /// "Whenever <filter> dies" — battlefield to graveyard specifically.
    Dies {
        who: ObjectFilter,
    },
    /// CR 702.35a — "when this card is exiled this way" (madness).
    ExiledForMadness {
        who: ObjectFilter,
    },
    /// Any zone change matching both endpoints.
    ZoneChange {
        who: ObjectFilter,
        from: Option<Zone>,
        to: Option<Zone>,
    },

    /// "At the beginning of <step>" — the turn-based trigger shape.
    StepBegins {
        step: Step,
        whose: Selector,
    },
    StepEnds {
        step: Step,
        whose: Selector,
    },

    Cast {
        who: ObjectFilter,
        by: Selector,
    },
    AbilityActivated {
        by: Selector,
    },
    /// "Whenever a player casts a spell that targets <you/this>".
    BecomesTarget {
        who: ObjectFilter,
        by: Selector,
    },

    Attacks {
        who: ObjectFilter,
    },
    Blocks {
        who: ObjectFilter,
    },
    BecomesBlocked {
        who: ObjectFilter,
    },
    /// "Whenever you sacrifice a Food", "whenever you sacrifice another permanent".
    Sacrificed {
        who: ObjectFilter,
        by: Selector,
    },
    /// "Whenever you scry" / "whenever you surveil".
    Scried {
        whose: Selector,
        surveil: bool,
    },
    /// "When you cycle ~", "whenever a player cycles a card" (CR 702.29).
    Cycled {
        who: ObjectFilter,
        by: Selector,
    },
    /// "Whenever ~ attacks and isn't blocked" (CR 509.3h): as blockers are declared, for
    /// each attacker no creature blocks.
    AttacksUnblocked {
        who: ObjectFilter,
    },

    DealsDamage {
        source: ObjectFilter,
        to: DamageRecipient,
        combat_only: bool,
    },
    TakesDamage {
        who: ObjectFilter,
        combat_only: bool,
    },

    LifeGained {
        whose: Selector,
    },
    LifeLost {
        whose: Selector,
    },
    Draws {
        whose: Selector,
    },
    Discards {
        whose: Selector,
    },

    BecomesTapped {
        who: ObjectFilter,
    },
    BecomesUntapped {
        who: ObjectFilter,
    },
    CounterPlaced {
        on: ObjectFilter,
        kind: CounterKind,
    },
    /// CR 714.2b — a Saga's chapter ability: lore counters on it went from fewer than
    /// `chapter` to at least `chapter`.
    ChapterReached {
        lore: CounterKind,
        chapter: u8,
    },
    /// "Whenever <attacker> becomes blocked by <blocker>": once per blocker, binding the
    /// blocker as the event's subject and the attacker as the other party.
    BlockedBy {
        attacker: ObjectFilter,
        blocker: ObjectFilter,
    },
    /// CR 701.37 — "when ~ becomes monstrous".
    BecomesMonstrous {
        who: ObjectFilter,
    },
    /// "Whenever you cast your second spell each turn": the controller's `n`th spell this
    /// turn.
    NthSpellCast {
        n: u32,
    },
    /// A reflexive trigger's own moment (see `Effect::Reflexive`).
    Reflexive,
    /// Dethrone (CR 702.105a): attacks the player with the most life or tied for most.
    AttacksMostLife {
        who: ObjectFilter,
    },
    /// Heroic: "whenever you cast a spell that targets ~".
    CastTargeting {
        by: Selector,
        target: ObjectFilter,
    },
    /// "Whenever you copy an instant or sorcery spell" (magecraft's other half).
    Copied {
        who: ObjectFilter,
        by: Selector,
    },
    /// "Whenever you draw your second card each turn".
    NthDraw {
        whose: Selector,
        n: u32,
    },
    /// CR 702.37 — "when ~ is turned face up".
    TurnedFaceUp {
        who: ObjectFilter,
    },
    /// CR 716.2c — "when this Class becomes level 2".
    BecomesClassLevel {
        level: u8,
    },
    /// "When the last time counter is removed from it" (suspend, CR 702.62a).
    LastCounterRemoved {
        from: ObjectFilter,
        kind: CounterKind,
    },

    /// A state trigger's condition rather than an event (CR 603.8).
    StateIs(Condition),

    /// Matches when any of several patterns match, for cards that list
    /// alternatives. Fires **once** per event even if several arms match.
    AnyOf(Vec<EventPattern>),
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum DamageRecipient {
    Player(Selector),
    Object(ObjectFilter),
    Any,
}

/// A boolean question about game state. Used for intervening-if clauses, state
/// triggers, and conditional effects.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Condition {
    Always,
    /// At least `at_least` objects match.
    CountAtLeast {
        what: Selector,
        at_least: Value,
    },
    CountAtMost {
        what: Selector,
        at_most: Value,
    },
    ValueAtLeast {
        lhs: Value,
        rhs: Value,
    },
    ValueEquals {
        lhs: Value,
        rhs: Value,
    },
    /// "if you control" / "if it is still on the battlefield".
    Exists(Selector),
    /// True during the listed step only.
    DuringStep(Step),
    /// True on the controller's own turn.
    YourTurn,
    /// CR 702.33d — the spell being resolved was kicked.
    Kicked,
    /// CR 702.31a — the source came under its controller's control since the beginning
    /// of that player's last upkeep (echo).
    ControlledSinceLastUpkeep,
    /// No spells were cast last turn (werewolves).
    NoSpellsLastTurn,
    /// A player cast two or more spells last turn (werewolves).
    PlayerCastTwoLastTurn,
    /// The source permanent is monstrous (CR 701.37b).
    Monstrous,
    /// The source is saddled (CR 702.171b).
    Saddled,
    /// The controller has max speed (CR 702.179f).
    MaxSpeed,
    /// The controller is the monarch (CR 724).
    YouAreMonarch,
    /// Morbid: a creature died this turn.
    CreatureDiedThisTurn,
    /// The source permanent was cast — "if you cast it" — from this zone, when given
    /// ("if you cast it from your hand").
    WasCast(Option<mtg_core::Zone>),
    /// The source Class is at least this level (CR 716.2); a Class starts at level 1.
    ClassLevelAtLeast(u8),
    /// The controller attacked with a creature this turn (raid).
    YouAttackedThisTurn,
    /// The source permanent is renowned (CR 702.112b).
    Renowned,
    /// The source permanent was cast for this alternative cost (evoke).
    CastFor(crate::ability::AltCost),
    /// An opponent of the controller was dealt damage this turn (bloodthirst).
    OpponentDamagedThisTurn,
    /// An opponent of the controller lost life this turn, by damage or otherwise.
    OpponentLostLifeThisTurn,
    /// The controller gained life this turn.
    YouGainedLifeThisTurn,
    /// A creature is attacking the controller or a planeswalker they control: "if you've
    /// been attacked this step".
    YouAreAttacked,
    /// The source spell targets an object matching this: "if it targets a tapped
    /// creature" (CR 601.2f — read from the targets chosen during announcement).
    TargetsMatching(crate::ObjectFilter),
    Not(Box<Condition>),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}
