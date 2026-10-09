//! Costs (CR 117, CR 601.2f-h).
//!
//! Costs are separate from effects because they are *paid*, not resolved: they
//! happen during announcement, they cannot be responded to, and failing to pay
//! rewinds the whole announcement rather than fizzling.

use mtg_core::{CounterKind, ManaCost, Zone};
use serde::{Deserialize, Serialize};

use crate::{
    selector::{ObjectFilter, Selector},
    trigger::Condition,
    value::Value,
};

#[derive(Clone, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
pub struct Cost {
    pub mana: ManaCost,
    pub additional: Vec<AdditionalCost>,
    /// Restrictions on *when* this may be paid at all (CR 601.2 timing checks).
    pub timing: Vec<Condition>,
}

impl Cost {
    pub fn free() -> Self {
        Self::default()
    }

    /// Costs with no game-state effect can be paid without a prompt when mana is
    /// unambiguous. The client policy layer uses this for one-click activation.
    pub fn is_trivial(&self) -> bool {
        self.additional.is_empty() && self.timing.is_empty()
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AdditionalCost {
    /// Marks the cost of a cycling ability (CR 702.29): paying it — discarding this card —
    /// is cycling it, which "when you cycle ~" sees. Costs nothing itself.
    Cycling,
    Tap {
        what: Selector,
    },
    Untap {
        what: Selector,
    },
    /// "Tap an untapped creature you control", "tap two untapped artifacts you control":
    /// objects chosen as the cost is paid (CR 601.2h). Unlike {T}, summoning sickness
    /// doesn't matter (CR 302.6).
    TapUntapped {
        filter: ObjectFilter,
        count: Value,
    },
    Sacrifice {
        what: Selector,
        count: Value,
    },
    /// "Discard your hand" (Lion's Eye Diamond): payable with any hand, even none.
    DiscardHand,
    Discard {
        count: Value,
        filter: ObjectFilter,
        at_random: bool,
    },
    ExileFrom {
        zone: Zone,
        filter: ObjectFilter,
        count: Value,
    },
    PayLife {
        amount: Value,
    },
    RemoveCounters {
        what: Selector,
        kind: CounterKind,
        amount: Value,
    },
    /// CR 606.4: a signed loyalty change paid during activation, including zero.
    Loyalty {
        delta: i32,
    },
    /// Reveal without moving — a cost that only proves something.
    Reveal {
        filter: ObjectFilter,
        count: Value,
    },
    /// "As an additional cost, choose ..." where the choice shapes the effect.
    ChooseMode,
    /// Crew (CR 702.122): "tap any number of untapped creatures you control with total
    /// power N or greater".
    TapCreaturesWithPower {
        power: Value,
    },
    /// Pay energy counters ("pay {E}{E}", CR 107.14).
    PayEnergy {
        amount: Value,
    },
    /// Ninjutsu (CR 702.49a): "return an unblocked attacking creature you control to its
    /// owner's hand". What it was attacking is remembered for the ability's effect.
    ReturnUnblockedAttacker,
    /// Return chosen permanents you control to their owners' hands during payment.
    ReturnToHand {
        filter: ObjectFilter,
        count: Value,
    },
    /// "Mill a card", "mill two cards" (CR 701.13b: only with that many in the library).
    Mill {
        count: Value,
    },
    /// "Put a -1/-1 counter on this creature": counters on the source.
    PutCounters {
        kind: CounterKind,
        amount: Value,
    },
    /// "Exert this creature" (CR 701.43): it doesn't untap during its controller's next
    /// untap step.
    Exert,
    /// The catch-all for costs carrying their own rules text.
    Native {
        key: Box<str>,
    },
}
