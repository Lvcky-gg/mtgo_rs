//! Vocabulary types shared by every other crate.
//!
//! This crate is deliberately inert: identifiers, zones, characteristics, mana,
//! and the event vocabulary. It contains no game logic, no I/O, and no policy.
//! Rule numbers in comments refer to the Magic Comprehensive Rules.

pub mod characteristics;
pub mod creature_types;
pub mod event;
pub mod ids;
pub mod mana;
pub mod zone;

pub use characteristics::{
    CardType, Characteristics, Color, ColorSet, GrantedAbility, Keyword, Subtype, Supertype,
};
pub use creature_types::is_creature_type;
pub use event::{
    Cause, CounterKind, DamageShield, Event, EventId, LossReason, ObjectDamageKind, StampedEvent,
    Step, Target,
};
pub use ids::{AbilityId, CardId, ObjectId, PlayerId, Timestamp};
pub use mana::{ManaCost, ManaPool, ManaSymbol};
pub use zone::{Zone, ZoneRef};
