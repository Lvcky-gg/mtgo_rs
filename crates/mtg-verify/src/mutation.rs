//! Controlled predicate mutation smoke campaigns. These do not claim to mutate
//! the production engine; use cargo-mutants for production mutation evidence.
use crate::differential::{CreatureSba, CreatureSbaInput, CreatureSbaOracle, RulesOracle};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Mutation {
    ZeroToughnessBoundary,
    DamageOneTooLow,
    IgnoreIndestructible,
    SkipSba,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MutationResult {
    pub mutation: Mutation,
    pub cases_run: u64,
    pub first_counterexample: Option<CreatureSbaInput>,
}
impl MutationResult {
    pub fn killed(&self) -> bool {
        self.first_counterexample.is_some()
    }
}

pub fn campaign(cases: &[CreatureSbaInput]) -> Vec<MutationResult> {
    [
        Mutation::ZeroToughnessBoundary,
        Mutation::DamageOneTooLow,
        Mutation::IgnoreIndestructible,
        Mutation::SkipSba,
    ]
    .into_iter()
    .map(|mutation| {
        let mut result = MutationResult {
            mutation,
            cases_run: 0,
            first_counterexample: None,
        };
        for &input in cases {
            result.cases_run += 1;
            let mut altered = input;
            let actual = match mutation {
                Mutation::ZeroToughnessBoundary => {
                    if altered.toughness == 0 {
                        altered.toughness = 1;
                    }
                    CreatureSbaOracle.evaluate(&altered)
                }
                Mutation::DamageOneTooLow => {
                    altered.marked_damage = altered.marked_damage.saturating_sub(1);
                    CreatureSbaOracle.evaluate(&altered)
                }
                Mutation::IgnoreIndestructible => {
                    altered.indestructible = false;
                    CreatureSbaOracle.evaluate(&altered)
                }
                Mutation::SkipSba => CreatureSba::Survives,
            };
            if actual != CreatureSbaOracle.evaluate(&input) {
                result.first_counterexample = Some(input);
                break;
            }
        }
        result
    })
    .collect()
}

/// None means no experiments, not a perfect score.
pub fn kill_rate(results: &[MutationResult]) -> Option<f64> {
    (!results.is_empty())
        .then(|| results.iter().filter(|r| r.killed()).count() as f64 / results.len() as f64)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supplied_counterexamples_kill_all_scoped_mutants() {
        let cases = [
            CreatureSbaInput {
                toughness: 0,
                marked_damage: 0,
                deathtouch_damage: false,
                indestructible: true,
            },
            CreatureSbaInput {
                toughness: 2,
                marked_damage: 2,
                deathtouch_damage: false,
                indestructible: false,
            },
            CreatureSbaInput {
                toughness: 2,
                marked_damage: 2,
                deathtouch_damage: false,
                indestructible: true,
            },
        ];
        assert_eq!(kill_rate(&campaign(&cases)), Some(1.0));
        assert_eq!(kill_rate(&[]), None);
    }
}
