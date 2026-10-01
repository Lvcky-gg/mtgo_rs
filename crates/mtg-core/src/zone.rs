//! Zones (CR 400).

use crate::PlayerId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Zone {
    Library,
    Hand,
    Battlefield,
    Graveyard,
    Stack,
    Exile,
    Command,
}

impl Zone {
    /// CR 400.2: shared zones have one instance for the whole game; player zones
    /// have one per player. This governs how `mtg-engine` keys its zone indices
    /// and how `PlayerView` redacts them.
    pub const fn is_shared(self) -> bool {
        matches!(
            self,
            Zone::Battlefield | Zone::Stack | Zone::Exile | Zone::Command
        )
    }

    /// Whether the *contents* are hidden from players other than the owner
    /// (CR 400.2). Face-down objects add per-object hiding on top of this.
    pub const fn is_hidden(self) -> bool {
        matches!(self, Zone::Library | Zone::Hand)
    }

    /// Whether order within the zone is part of the game state (CR 400.4).
    pub const fn is_ordered(self) -> bool {
        matches!(self, Zone::Library | Zone::Stack | Zone::Graveyard)
    }
}

/// A concrete zone instance: shared zones carry no player.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct ZoneRef {
    pub zone: Zone,
    pub player: Option<PlayerId>,
}

impl ZoneRef {
    pub const fn shared(zone: Zone) -> Self {
        Self { zone, player: None }
    }
    pub const fn of(zone: Zone, player: PlayerId) -> Self {
        Self {
            zone,
            player: Some(player),
        }
    }
}
