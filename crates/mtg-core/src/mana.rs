//! Mana symbols, costs, and pools (CR 106, CR 202).

use crate::characteristics::Color;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum ManaSymbol {
    Generic(u8),
    /// `{X}` — value chosen on announcement (CR 601.2b).
    Variable,
    Colored(Color),
    Colorless,
    Snow,
    /// `{W/U}` and friends: payable as either half.
    Hybrid(Color, Color),
    /// `{2/W}`: payable as generic 2 or one colored.
    MonoHybrid(u8, Color),
    /// `{W/P}`: payable as mana or 2 life (CR 107.4f).
    Phyrexian(Color),
}

#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ManaCost {
    pub symbols: Vec<ManaSymbol>,
}

impl ManaCost {
    pub const FREE: Self = Self {
        symbols: Vec::new(),
    };

    /// CR 202.3. `{X}` counts as 0 outside the stack.
    pub fn mana_value(&self) -> u32 {
        self.symbols
            .iter()
            .map(|s| match s {
                ManaSymbol::Generic(n) => u32::from(*n),
                ManaSymbol::MonoHybrid(n, _) => u32::from(*n),
                ManaSymbol::Variable => 0,
                ManaSymbol::Hybrid(..) => 1,
                _ => 1,
            })
            .sum()
    }
}

/// Mana available to spend. Emptied at the end of every step and phase (CR 500.4).
#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ManaPool {
    /// Indexed by `Color as usize`, plus one slot for colorless.
    pub amounts: [u16; 6],
}

impl ManaPool {
    pub const COLORLESS_SLOT: usize = 5;

    pub fn add(&mut self, color: Option<Color>, n: u16) {
        let slot = color.map_or(Self::COLORLESS_SLOT, |c| c as usize);
        self.amounts[slot] += n;
    }

    pub fn total(&self) -> u16 {
        self.amounts.iter().sum()
    }

    pub fn clear(&mut self) {
        self.amounts = [0; 6];
    }
}
