use crate::{
    canonical::CanonicalGameState,
    scenario::{GameScenario, ScenarioAction, ScenarioAssertions, next_choice, run},
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
    for _ in 0..steps {
        let choice = match next_choice(&mut engine, &cards) {
            Ok(Some(choice)) => choice,
            Ok(None) => break,
            Err(e) => {
                failure = Some(e);
                break;
            }
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
            expected_choice: Some(choice.kind),
            expected_state: None,
            expected_digest: None,
        });
        if let Err(e) = engine.answer(&cards, choice.id, answer) {
            failure = Some(format!("Offered/generated answer rejected: {e:?}"));
            break;
        }
        match next_choice(&mut engine, &cards) {
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
        if let Err(e) = crate::invariants::verify(&engine.state, &cards) {
            failure = Some(e);
            break;
        }
    }
    if failure.is_none() {
        artifact.expected.final_state = Some(CanonicalGameState::from_engine(&engine)?);
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
        reproduction: artifact,
    })
}

/// Delta-debug actions while retaining the same failure message; illegal prefixes
/// and changed failure classes are rejected, rather than counted as reductions.
pub fn minimize(input: &GameScenario, budget: usize) -> Result<GameScenario, String> {
    let (report, _) = run(input, false)?;
    if report.pass {
        return Err("Cannot minimize a passing scenario".into());
    }
    let signature = report.message;
    let fails = |candidate: &GameScenario| -> bool {
        run(candidate, false).is_ok_and(|(r, _)| !r.pass && r.message == signature)
    };
    let mut result = input.clone();
    let mut attempts = 0;
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
    // Setup reductions preserve existing action identities by trimming trailing objects.
    while attempts < budget && !result.initial_state.objects.is_empty() {
        let mut candidate = result.clone();
        candidate.initial_state.objects.pop();
        attempts += 1;
        if fails(&candidate) {
            result = candidate;
        } else {
            break;
        }
    }
    Ok(result)
}
