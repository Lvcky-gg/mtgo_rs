//! Access to printed card data.
//!
//! The trait lives here rather than in the rules engine because it is about *card
//! representation*, and its implementors are card-data crates. Putting it in the engine
//! would force anything that merely supplies cards to depend on the rules — the wrong way
//! round for a crate whose whole job is to hand data over.

use mtg_core::{CardId, Subtype};

/// Somewhere printed card data can be read from.
///
/// Implemented by an in-memory library loaded from the local database, and by the test
/// fixtures. Returning references means implementors hold their data rather than
/// constructing it per call, which is why the engine plays from an in-memory snapshot and
/// not directly from SQLite.
pub trait PrintedCards {
    /// One face of a card. Index 0 is the front.
    fn face(&self, card: CardId, face: u8) -> Option<&crate::CardFace>;

    /// How this card's faces relate. Existing single-face sources default to normal.
    /// Multi-face sources must preserve their imported layout explicitly.
    fn layout(&self, _card: CardId) -> crate::Layout {
        crate::Layout::Normal
    }

    /// Combined off-stack characteristics of a split card (CR 709.4).
    /// Casting still uses each half's own face and ability ids.
    fn split_characteristics(&self, card: CardId) -> Option<mtg_core::Characteristics> {
        let left = self.face(card, 0)?;
        let right = self.face(card, 1)?;
        let mut ch = left.printed_characteristics();
        let other = right.printed_characteristics();
        ch.name = format!("{} // {}", left.name, right.name).into();
        ch.mana_cost.symbols.extend(other.mana_cost.symbols);
        ch.colors = if left.colors.as_ref().is_some_and(Vec::is_empty)
            || right.colors.as_ref().is_some_and(Vec::is_empty)
        {
            mtg_core::ColorSet::COLORLESS
        } else {
            ch.colors.union(other.colors)
        };
        for ty in other.card_types {
            if !ch.card_types.contains(&ty) {
                ch.card_types.push(ty);
            }
        }
        for ty in other.subtypes {
            if !ch.subtypes.contains(&ty) {
                ch.subtypes.push(ty);
            }
        }
        for ty in other.supertypes {
            if !ch.supertypes.contains(&ty) {
                ch.supertypes.push(ty);
            }
        }
        let offset = u16::try_from(left.abilities.len()).ok()?;
        for ability in other.abilities {
            ch.abilities
                .push(mtg_core::AbilityId(offset.checked_add(ability.0)?));
        }
        Some(ch)
    }

    /// The printed name of an interned subtype.
    ///
    /// Subtypes are interned integers so the engine can compare them cheaply, but a few
    /// rules name one directly — the Aura check in CR 704.5m is the one that matters.
    /// Resolving through the database keeps those strings out of the binary along with the
    /// rest of the card data.
    fn subtype_name(&self, subtype: Subtype) -> Option<&str>;

    /// The interned id of a subtype by name, for the few rules that add one (a bestowed
    /// Aura becomes an Aura). `None` where the table cannot say.
    fn subtype_named(&self, _name: &str) -> Option<Subtype> {
        None
    }
}
