//! The card intermediate representation.
//!
//! Cards are **data, not code**. Every card is a list of abilities, and every
//! ability is a tree built from a closed set of primitives defined here. Nothing
//! in this crate executes: `mtg-engine` interprets it, `mtg-cards` stores it as
//! on-disk RON, and `mtg-oracle` maps printed oracle text onto it.
//!
//! Three properties fall out of making cards data, and all three are the reason
//! for this design:
//!
//! 1. **Adding a card ships no binary.** A card is a file. Card fixes and new
//!    sets do not require a release, and a contributor does not need to build
//!    the project to add a card.
//! 2. **Abilities are statically analysable.** [`footprint`] derives what an
//!    effect reads and writes *without running it*, which is what lets the
//!    engine prove two triggers cannot interact and skip asking the player to
//!    order them. That analysis is impossible over arbitrary code.
//! 3. **Abilities are explainable.** The same tree that resolves also renders
//!    the "why is this on the stack?" trace, because the trace is just the
//!    matched [`trigger::EventPattern`] plus the event that matched it.
//!
//! The escape hatch is [`ability::AbilityKind::Native`]: a handful of cards
//! genuinely rewrite the rules, and they get Rust implementations registered by
//! oracle id. The rule of thumb is that reaching for `Native` when the IR could
//! express the card is a bug, because a `Native` ability is opaque to the two
//! analyses above and will therefore always prompt.

pub mod ability;
pub mod cards;
pub mod cost;
pub mod effect;
pub mod footprint;
pub mod selector;
pub mod trigger;
pub mod value;
pub mod walk;

pub use ability::{Ability, AbilityKind, CardDefinition, CardFace, Layout};
pub use cards::PrintedCards;
pub use cost::{AdditionalCost, Cost};
pub use effect::{Effect, ManaOutput, Replacement};
pub use footprint::{Footprint, Resource};
pub use selector::{ObjectFilter, Selector, TargetSpec};
pub use trigger::{EventPattern, Trigger, TriggerTiming};
pub use value::Value;

/// The colors a mana cost implies (CR 202.2).
///
/// A card's printed color comes from its mana cost unless a characteristic-defining
/// ability says otherwise; the layer system handles the exceptions.
pub fn color_of(cost: &mtg_core::ManaCost) -> mtg_core::ColorSet {
    use mtg_core::{ColorSet, ManaSymbol};
    cost.symbols
        .iter()
        .fold(ColorSet::COLORLESS, |acc, s| match s {
            ManaSymbol::Colored(c) | ManaSymbol::MonoHybrid(_, c) | ManaSymbol::Phyrexian(c) => {
                acc.union(ColorSet::single(*c))
            }
            ManaSymbol::Hybrid(a, b) => acc.union(ColorSet::single(*a)).union(ColorSet::single(*b)),
            _ => acc,
        })
}
