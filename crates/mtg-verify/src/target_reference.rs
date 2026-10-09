//! Independent scoped targeting decision table. No production filter/layer
//! evaluator is called. Covers battlefield creatures, owner/controller filters,
//! shroud, hexproof, phasing and distinct target slots in a two-player game.
//! Protection, player targets, continuous effects, copied characteristics,
//! stack self-targeting and arbitrary IR filters are outside this model.
use mtg_core::{ObjectId, PlayerId, Zone};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Relation {
    Any,
    You,
    Opponent,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub id: ObjectId,
    pub zone: Zone,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub creature: bool,
    pub phased_out: bool,
    pub shroud: bool,
    pub hexproof: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetProblem {
    pub source_controller: PlayerId,
    pub owner: Relation,
    pub controller: Relation,
    pub distinct: bool,
    pub already: Vec<ObjectId>,
    pub candidates: Vec<Candidate>,
}
fn relates(relation: Relation, player: PlayerId, source_controller: PlayerId) -> bool {
    match relation {
        Relation::Any => true,
        Relation::You => player == source_controller,
        Relation::Opponent => player != source_controller,
    }
}
impl TargetProblem {
    pub fn validate(&self) -> Result<(), String> {
        if self.source_controller.0 > 1 || self.candidates.len() > 32 {
            return Err("Scoped targeting reference requires bounded two-player inputs".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for candidate in &self.candidates {
            if candidate.owner.0 > 1 || candidate.controller.0 > 1 || !seen.insert(candidate.id) {
                return Err("Unsupported player identity or duplicate object identity".into());
            }
        }
        Ok(())
    }
}
pub struct TargetOracle;
impl crate::differential::RulesOracle<TargetProblem> for TargetOracle {
    type Output = Result<Vec<ObjectId>, String>;
    fn evaluate(&self, input: &TargetProblem) -> Self::Output {
        input.validate()?;
        let mut accepted = Vec::new();
        for object in &input.candidates {
            // These are independent rules propositions, not the production
            // ObjectFilter::Targetable branch or a translation of its IR tree.
            let exists_for_rules = object.zone == Zone::Battlefield && !object.phased_out;
            let satisfies_noun = object.creature
                && relates(input.owner, object.owner, input.source_controller)
                && relates(input.controller, object.controller, input.source_controller);
            let blocked_by_keyword = matches!(
                (
                    object.shroud,
                    object.hexproof,
                    object.controller == input.source_controller
                ),
                (true, _, _) | (false, true, false)
            );
            let slot_collision = input.distinct && input.already.contains(&object.id);
            if exists_for_rules && satisfies_noun && !blocked_by_keyword && !slot_collision {
                accepted.push(object.id);
            }
        }
        accepted.sort();
        Ok(accepted)
    }
    fn provenance(&self) -> &'static str {
        "Independent scoped CR 115/601.2c/702.18/702.11/702.26 decision table; two-player plain battlefield creatures only; unsupported identities return errors; independent review still required"
    }
}
