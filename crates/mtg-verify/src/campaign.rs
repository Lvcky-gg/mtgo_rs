use crate::{
    canonical::CanonicalGameState,
    scenario::{
        GameScenario, RunReport, ScenarioAction, ScenarioAssertions, next_choice_with_budget, run,
    },
};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
    state::Rng,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CampaignReport {
    pub seed: u64,
    pub actions: usize,
    pub failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed: Option<RunReport>,
    pub reproduction: GameScenario,
}

/// Consume offered actions, not an alternative implementation of casting/combat.
/// Random priority selection and policy-generated legal complex choices are recorded.
pub fn semantic(base: &GameScenario, seed: u64, steps: usize) -> Result<CampaignReport, String> {
    if steps > crate::scenario::MAX_ACTIONS {
        return Err("Campaign action limit exceeded".into());
    }
    let mut artifact = base.clone();
    artifact.seed = seed;
    artifact.actions.clear();
    artifact.expected = ScenarioAssertions::default();
    let (mut engine, cards) = artifact.setup()?;
    let mut bytes = [0; 32];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    let mut rng = Rng::from_seed(&bytes);
    let mut failure = None;
    // Replay normalizes setup before consuming answers, including empty histories.
    // Campaigns must capture that same stable/terminal boundary for steps=0 too.
    let mut pending = match next_choice_with_budget(&mut engine, &cards, artifact.advance_budget) {
        Ok(choice) => choice,
        Err(error) => {
            failure = Some(error);
            None
        }
    };
    if failure.is_none()
        && let Err(error) = crate::invariants::verify(&engine.state, &cards)
    {
        failure = Some(error);
    }
    for _ in 0..steps {
        if failure.is_some() {
            break;
        }
        let Some(choice) = pending.take() else {
            break;
        };
        let answer = if let ChoiceKind::Priority { legal } = &choice.kind {
            let offered: Vec<_> = legal
                .actions
                .iter()
                .chain(&legal.mana_abilities)
                .filter(|a| !matches!(a, Action::Concede))
                .collect();
            if offered.is_empty() {
                Answer::Pass
            } else {
                Answer::Action(offered[rng.next_u64() as usize % offered.len()].clone())
            }
        } else {
            mtg_policy::well_formed(&choice, &engine.view_for(choice.who))
        };
        artifact.actions.push(ScenarioAction {
            who: choice.who,
            answer: answer.clone(),
            expected_rejection: false,
            expected_choice: Some(choice.kind),
            expected_state: None,
            expected_digest: None,
        });
        if let Err(e) = engine.answer(&cards, choice.id, answer) {
            failure = Some(format!("Illegal recorded answer: {e:?}"));
            break;
        }
        pending = match next_choice_with_budget(&mut engine, &cards, artifact.advance_budget) {
            Ok(choice) => choice,
            Err(e) => {
                failure = Some(e);
                break;
            }
        };
        if let Err(e) = crate::invariants::verify(&engine.state, &cards) {
            failure = Some(e);
            break;
        }
    }
    let state = CanonicalGameState::from_engine(&engine)?;
    let observed = if let Some(error) = &failure {
        let mut report = crate::scenario::failure(0, error, None, state.clone());
        report.first_divergent_action = artifact.actions.len().checked_sub(1);
        report
    } else {
        RunReport {
            pass: true,
            first_divergent_action: None,
            message: "PASS".into(),
            digest: state.digest(),
            expected_state: None,
            actual_state: Some(state.clone()),
            diff: vec![],
        }
    };
    if failure.is_none() {
        artifact.expected.final_state = Some(state);
        artifact.expected.final_digest = artifact
            .expected
            .final_state
            .as_ref()
            .map(CanonicalGameState::digest);
    }
    artifact.metadata.description = format!(
        "Semantic campaign seed {seed}; {}",
        failure.as_deref().unwrap_or("completed")
    );
    Ok(CampaignReport {
        seed,
        actions: artifact.actions.len(),
        failure,
        observed: Some(observed),
        reproduction: artifact,
    })
}

/// Observable failure identity, independent of its position in the action list.
/// Checkpoint expectations and diff paths prevent stale checkpoint substitution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureSignature {
    pub message: String,
    pub action_digest: Option<String>,
    pub diff_paths: Vec<String>,
}
fn signature(scenario: &GameScenario, report: &crate::scenario::RunReport) -> FailureSignature {
    let canonical_failure = matches!(
        report.message.as_str(),
        "First divergent checkpoint" | "Final state differs"
    );
    FailureSignature {
        // Actual totals are diagnostics, not the identity of a zone assertion.
        message: report
            .message
            .split(", actual ")
            .next()
            .unwrap_or(&report.message)
            .into(),
        action_digest: report
            .first_divergent_action
            .and_then(|index| scenario.actions.get(index))
            .map(|action| {
                crate::canonical::hash(&serde_json::to_vec(action).expect("action serializes"))
            }),
        diff_paths: if canonical_failure {
            report
                .diff
                .iter()
                .map(|line| line.split(": expected ").next().unwrap_or(line).into())
                .collect()
        } else {
            vec![]
        },
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MinimizationReport {
    pub attempts: usize,
    pub reductions: usize,
    pub budget_exhausted: bool,
    pub original_actions: usize,
    pub original_objects: usize,
    pub failure: FailureSignature,
    pub reproduction: GameScenario,
}

/// Minimize against the same observable oracle. No checkpoints are recorded or
/// assertions rewritten. A passing or differently failing candidate is rejected.
/// These candidates still need independent rules review before regression intake.
pub fn minimize(input: &GameScenario, budget: usize) -> Result<GameScenario, String> {
    Ok(minimize_with_report(input, budget)?.reproduction)
}

pub fn minimize_with_report(
    input: &GameScenario,
    budget: usize,
) -> Result<MinimizationReport, String> {
    let (report, _) = run(input, false)?;
    if report.pass {
        return Err("Cannot minimize a passing scenario".into());
    }
    let expected = signature(input, &report);
    let fails = |candidate: &GameScenario| -> bool {
        run(candidate, false).is_ok_and(|(r, _)| !r.pass && signature(candidate, &r) == expected)
    };
    let mut result = input.clone();
    let mut attempts = 0;
    let mut reductions = 0;
    let mut chunk = (result.actions.len() / 2).max(1);
    while attempts < budget {
        let mut changed = false;
        let mut start = 0;
        while start < result.actions.len() && attempts < budget {
            let mut candidate = result.clone();
            candidate
                .actions
                .drain(start..(start + chunk).min(candidate.actions.len()));
            attempts += 1;
            if fails(&candidate) {
                result = candidate;
                reductions += 1;
                changed = true;
            } else {
                start += chunk;
            }
        }
        if !changed {
            if chunk == 1 {
                break;
            }
            chunk = (chunk / 2).max(1);
        }
    }
    // Without actions, removing any initial object cannot rebind an action ID.
    // With actions, conservatively retain the existing trailing-only strategy.
    let mut index = result.initial_state.objects.len();
    while attempts < budget && index > 0 {
        index -= 1;
        let mut candidate = result.clone();
        candidate.initial_state.objects.remove(index);
        attempts += 1;
        if fails(&candidate) {
            result = candidate;
            reductions += 1;
        } else if !result.actions.is_empty() {
            break;
        }
    }
    // Simplify object-local setup without changing allocation or ownership.
    for index in 0..result.initial_state.objects.len() {
        if attempts >= budget {
            break;
        }
        let original = result.initial_state.objects[index].clone();
        let mut candidates = Vec::new();
        if original.damage != 0 {
            let mut object = original.clone();
            object.damage = 0;
            candidates.push(object);
        }
        if original.tapped {
            let mut object = original.clone();
            object.tapped = false;
            candidates.push(object);
        }
        for counter in original.counters.keys() {
            let mut object = original.clone();
            object.counters.remove(counter);
            candidates.push(object);
        }
        for object in candidates {
            if attempts >= budget {
                break;
            }
            let mut candidate = result.clone();
            // Keep already accepted simplifications when changing one local field.
            let target = &mut candidate.initial_state.objects[index];
            if object.damage != original.damage {
                target.damage = object.damage;
            }
            if object.tapped != original.tapped {
                target.tapped = object.tapped;
            }
            for counter in original.counters.keys() {
                if !object.counters.contains_key(counter) {
                    target.counters.remove(counter);
                }
            }
            attempts += 1;
            if fails(&candidate) {
                result = candidate;
                reductions += 1;
            }
        }
    }
    Ok(MinimizationReport {
        attempts,
        reductions,
        budget_exhausted: attempts >= budget,
        original_actions: input.actions.len(),
        original_objects: input.initial_state.objects.len(),
        failure: expected,
        reproduction: result,
    })
}
