//! Dynamic numbers.
//!
//! Anything that could be "2" or "the number of creatures you control" is a
//! `Value`. Keeping counts symbolic rather than resolving them at parse time is
//! what makes the footprint analysis honest: `Value::Count(..)` declares a read.

use crate::selector::Selector;
use mtg_core::CounterKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Value {
    Fixed(i32),
    /// The value chosen for `{X}` when the spell was cast (CR 601.2b).
    X,
    /// "That much", "that many": the amount of the event that triggered the ability —
    /// damage dealt, life gained or lost. Fixed as the ability triggers (CR 603.3a).
    EventAmount,
    /// Sunburst, converge: how many colors of mana were spent to cast the source.
    ColorsSpent,
    /// Mana of this color spent to cast the source ("if {R} was spent to cast it",
    /// adamant).
    ManaSpentOfColor(mtg_core::Color),
    /// How many objects a selector currently matches.
    Count(Box<Selector>),
    /// Life total of the player a selector picks out.
    LifeTotal(Box<Selector>),
    /// Counters of one kind on the objects a selector picks out.
    Counters(Box<Selector>, CounterKind),
    /// A characteristic of an object, read through the layer system.
    Power(Box<Selector>),
    Toughness(Box<Selector>),
    ManaValue(Box<Selector>),
    /// Arithmetic, kept explicit so footprints compose.
    Sum(Vec<Value>),
    Product(Vec<Value>),
    Negate(Box<Value>),
    /// `max(a, b)` — the common "but not less than 0" shape.
    Max(Box<Value>, Box<Value>),
    Min(Box<Value>, Box<Value>),
    /// The least toughness among the objects a selector picks out (bolster, CR 701.39a);
    /// 0 when it picks out none.
    LeastToughness(Box<Selector>),
    /// How many card types there are among the objects a selector picks out (delirium).
    CardTypesAmong(Box<Selector>),
    /// CR 702.40a — how many spells were cast before the source spell this turn (storm).
    SpellsCastBefore,
    /// CR 700.8 — the number of creatures in a party among these: up to one each of
    /// Cleric, Rogue, Warrior and Wizard.
    PartySize(Box<Selector>),
    /// Domain: how many basic land types (Plains, Island, Swamp, Mountain, Forest) appear
    /// among these.
    BasicLandTypesAmong(Box<Selector>),
    /// How many spells these players have cast this turn.
    SpellsCastThisTurn(Box<Selector>),
    /// A number the controller announces on resolution, within bounds.
    ChosenByController {
        min: Box<Value>,
        max: Box<Value>,
    },
}

/// `Value::ONE`, for serde defaults.
pub fn one() -> Value {
    Value::ONE
}

impl Value {
    pub const ZERO: Self = Value::Fixed(0);
    pub const ONE: Self = Value::Fixed(1);

    /// Whether this value can be computed without looking at game state, which
    /// means an effect using it reads nothing.
    pub fn is_static(&self) -> bool {
        match self {
            Value::Fixed(_) => true,
            Value::Sum(vs) | Value::Product(vs) => vs.iter().all(Value::is_static),
            Value::Negate(v) => v.is_static(),
            Value::Max(a, b) | Value::Min(a, b) => a.is_static() && b.is_static(),
            _ => false,
        }
    }
}
