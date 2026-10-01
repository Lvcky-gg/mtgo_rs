//! Identifier newtypes. All of them are opaque, `Copy`, and ordered so that the
//! engine can use them as deterministic tie-breakers.

use serde::{Deserialize, Serialize};

macro_rules! id_newtype {
    ($(#[$m:meta])* $name:ident($inner:ty)) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub $inner);

        impl $name {
            pub const fn raw(self) -> $inner { self.0 }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

id_newtype! {
    /// Identity of a *game object* — a card, token, copy, or ability on the stack.
    ///
    /// CR 400.7: an object that changes zones becomes a **new** object. The engine
    /// therefore allocates a fresh `ObjectId` on every zone change rather than
    /// carrying one identity across zones. Abilities that need to look backwards
    /// ("leaves-the-battlefield" triggers, CR 603.10) read last-known information
    /// from the event log, never from a stale id.
    ObjectId(u32)
}

id_newtype! {
    /// A seat at the table, stable for the whole game. Turn order is ascending.
    PlayerId(u8)
}

id_newtype! {
    /// A printing-independent oracle identity: the key into the local card database.
    ///
    /// Interned from Scryfall's `oracle_id` at import. No card text is compiled
    /// into this program; see `mtg-oracle`.
    CardId(u32)
}

id_newtype! {
    /// Identifies one ability *within* a card's ability list.
    AbilityId(u16)
}

id_newtype! {
    /// Monotonic counter used for CR 613.7 timestamp order.
    ///
    /// Bumped on every state mutation, so it doubles as the cache generation for
    /// computed characteristics.
    Timestamp(u64)
}

impl AbilityId {
    /// Ids from here up name abilities granted by layer 6 effects: the index into
    /// [`crate::Characteristics::granted_abilities`], offset. Printed ability lists are
    /// nowhere near this long, so the two ranges never meet.
    pub const GRANTED_BASE: u16 = 0x8000;
    /// The ward {2} of a face-down disguised permanent (CR 702.168a), which has no
    /// printed abilities to number it among.
    pub const DISGUISE_WARD: Self = Self(0x7FFE);

    pub const fn granted(index: usize) -> Self {
        Self(Self::GRANTED_BASE + index as u16)
    }

    /// For a granted ability, its index into the granted list.
    pub const fn granted_index(self) -> Option<usize> {
        if self.0 >= Self::GRANTED_BASE {
            Some((self.0 - Self::GRANTED_BASE) as usize)
        } else {
            None
        }
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Timestamp {
    pub const ZERO: Self = Self(0);

    /// Bump and return the new value.
    pub fn advance(&mut self) -> Self {
        self.0 += 1;
        *self
    }
}
