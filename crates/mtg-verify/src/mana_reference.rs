//! Slow independent feasibility oracle: enumerate each source's concrete color
//! choice, then check pool capacities. Production uses augmenting-path matching.
//! Scope: fixed floating mana; distinct unrestricted sources producing exactly
//! one mana; plain colored, colorless and generic costs. No hybrid, phyrexian,
//! snow, X, restrictions, life payment, activation costs, convoke or correlated
//! multi-mana outputs. Out-of-scope input is an error, never an approximation.
use mtg_core::Color;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManaProblem {
    pub pool: [u16; 6],
    pub required: [u16; 6],
    pub generic: u16,
    /// None is colorless. Each source is a distinct, freely activatable one-mana
    /// ability. A source may offer colored choices OR colorless, not both.
    pub sources: Vec<Vec<Option<Color>>>,
}

impl ManaProblem {
    pub fn validate(&self) -> Result<(), String> {
        if self.sources.len() > 6
            || self.pool.iter().chain(&self.required).any(|n| *n > 32)
            || self.generic > 32
        {
            return Err("Reference mana problem exceeds bounded enumeration scope".into());
        }
        for choices in &self.sources {
            if choices.is_empty()
                || choices.len() > 5
                || (choices.contains(&None) && choices.len() != 1)
            {
                return Err("Unsupported source choices".into());
            }
        }
        Ok(())
    }
    pub fn feasible(&self) -> Result<bool, String> {
        self.validate()?;
        let available = self.pool.map(u32::from);
        Ok(self.enumerate(0, available))
    }
    fn enumerate(&self, source: usize, available: [u32; 6]) -> bool {
        if source == self.sources.len() {
            let mut surplus = 0_u32;
            for (available, required) in available.iter().zip(self.required) {
                let Some(remainder) = available.checked_sub(u32::from(required)) else {
                    return false;
                };
                surplus += remainder;
            }
            return surplus >= u32::from(self.generic);
        }
        self.sources[source].iter().any(|color| {
            let mut next = available;
            next[color.map_or(5, |color| color as usize)] += 1;
            self.enumerate(source + 1, next)
        })
    }
}

/// Adapter for the shared differential API. Validation errors explicitly mark
/// unsupported inputs instead of pretending they are unpayable Magic costs.
pub struct ManaOracle;
impl crate::differential::RulesOracle<ManaProblem> for ManaOracle {
    type Output = Result<bool, String>;
    fn evaluate(&self, problem: &ManaProblem) -> Self::Output {
        problem.feasible()
    }
    fn provenance(&self) -> &'static str {
        "Independent bounded exhaustive source-color enumeration; plain colored/colorless/generic, one mana per unrestricted source; unsupported inputs return errors; review metadata is advisory"
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::differential::RulesOracle;
    #[test]
    fn oracle_adapter_preserves_out_of_scope_errors() {
        let input = ManaProblem {
            pool: [0; 6],
            required: [0; 6],
            generic: 0,
            sources: vec![vec![]],
        };
        assert!(ManaOracle.evaluate(&input).is_err());
    }
}
