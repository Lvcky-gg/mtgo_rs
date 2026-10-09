//! Independent failure-oracle preservation tests; reductions must not substitute
//! an earlier stale checkpoint or rejected answer for the reported failure.
use mtg_core::{CardId, PlayerId};
use mtg_verify::{
    campaign::minimize,
    scenario::{GameScenario, run},
};
use std::path::PathBuf;
fn basic() -> GameScenario {
    GameScenario::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/replays/basic_casting.json"),
    )
    .unwrap()
}
fn life_failure() -> GameScenario {
    let mut s = basic();
    s.expected.zones.clear();
    s.expected.life.insert(PlayerId(0), 999);
    s
}
#[test]
fn zero_budget_preserves_complete_artifact_byte_for_byte() {
    let s = life_failure();
    assert_eq!(
        serde_json::to_value(minimize(&s, 0).unwrap()).unwrap(),
        serde_json::to_value(s).unwrap()
    );
}
#[test]
fn issue_and_rules_metadata_survive_reduction_without_rewriting_oracle() {
    let mut s = life_failure();
    s.metadata.issue = Some("independent-oracle-identity".into());
    s.metadata.description = "Life assertion must survive minimization".into();
    s.metadata.rules = vec!["119.3".into()];
    s.metadata.first_affected = Some("old-build".into());
    s.metadata.fixed_in = Some("review-build".into());
    let reduced = minimize(&s, 64).unwrap();
    assert_eq!(
        serde_json::to_value(&reduced.metadata).unwrap(),
        serde_json::to_value(&s.metadata).unwrap()
    );
    assert_eq!(reduced.expected.life, s.expected.life);
    let report = run(&reduced, false).unwrap().0;
    assert!(!report.pass);
    assert!(report.message.contains("life 999"));
}
#[test]
fn last_checkpoint_failure_cannot_be_replaced_by_an_earlier_stale_checkpoint() {
    let s = GameScenario::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/minimization/checkpoint_substitution.json"),
    )
    .unwrap();
    let last = s.actions.len() - 1;
    assert_eq!(run(&s, false).unwrap().0.first_divergent_action, Some(last));
    let reduced = minimize(&s, 64).unwrap();
    let report = run(&reduced, false).unwrap().0;
    assert!(!report.pass);
    let failed_action = &reduced.actions[report.first_divergent_action.unwrap()];
    assert_eq!(
        failed_action.expected_digest.as_deref(),
        Some("independent-final-action-oracle"),
        "same generic checkpoint message must not substitute a different observable failure"
    );
}
#[test]
fn stale_checkpoints_do_not_block_valid_life_assertion_reduction() {
    let s = life_failure();
    let reduced = minimize(&s, 64).unwrap();
    assert!(reduced.actions.len() < s.actions.len());
    let report = run(&reduced, false).unwrap().0;
    assert!(!report.pass);
    assert!(report.message.contains("life 999"));
}
#[test]
fn middle_object_reduction_must_not_rebind_cast_to_a_different_card() {
    let mut s = life_failure();
    // Place the referenced spell after an irrelevant initial object. Removing that
    // middle/prefix object requires remapping every action identity or declining
    // the reduction; otherwise ObjectId(2) starts referring to a library creature.
    s.expected.zones.push(mtg_verify::scenario::ZoneAssertion {
        zone: mtg_core::ZoneRef::of(mtg_core::Zone::Graveyard, PlayerId(0)),
        card: Some(CardId(1)),
        owner: Some(PlayerId(0)),
        count: 1,
        counters: Default::default(),
    });
    let spell = s.initial_state.objects.remove(0);
    s.initial_state.objects.insert(1, spell);
    let mtg_engine::Answer::Action(mtg_engine::Action::Cast { object }) = &mut s.actions[0].answer
    else {
        panic!("cast fixture")
    };
    *object = mtg_core::ObjectId(2);
    for action in &mut s.actions {
        action.expected_digest = None;
        action.expected_state = None;
        action.expected_choice = None;
    }
    s.expected.final_digest = None;
    s.expected.final_state = None;
    assert!(run(&s, false).unwrap().0.message.contains("life 999"));
    let reduced = minimize(&s, 64).unwrap();
    let report = run(&reduced, false).unwrap().0;
    assert!(
        report.message.contains("life 999"),
        "cannot substitute illegal answer failure"
    );
    assert!(
        reduced.actions.iter().any(|action| matches!(
            action.answer,
            mtg_engine::Answer::Action(mtg_engine::Action::Cast { .. })
        )),
        "zone proposition requires preserving the actual casting transition"
    );
    for action in &reduced.actions {
        if let mtg_engine::Answer::Action(mtg_engine::Action::Cast { object }) = action.answer {
            let (engine, _) = reduced.setup().unwrap();
            assert_eq!(engine.state.objects[&object].card, CardId(1));
        }
    }
}

#[test]
fn bounded_report_counts_attempts_and_never_exceeds_budget() {
    use mtg_verify::campaign::minimize_with_report;
    let s = life_failure();
    for budget in [0, 1, 2, 3, 8] {
        let report = minimize_with_report(&s, budget).unwrap();
        assert!(report.attempts <= budget);
        assert!(report.reductions <= report.attempts);
        assert_eq!(report.original_actions, s.actions.len());
        assert_eq!(report.original_objects, s.initial_state.objects.len());
        assert_eq!(report.budget_exhausted, report.attempts == budget);
        assert!(
            run(&report.reproduction, false)
                .unwrap()
                .0
                .message
                .contains("life 999")
        );
        if budget == 0 {
            assert_eq!(report.reductions, 0);
            assert_eq!(
                serde_json::to_value(&report.reproduction).unwrap(),
                serde_json::to_value(&s).unwrap()
            );
        }
    }
}

#[test]
fn local_setup_simplifications_preserve_counter_oracle_and_required_object() {
    use mtg_core::{CounterKind, Zone, ZoneRef};
    use mtg_verify::campaign::minimize_with_report;
    use mtg_verify::scenario::{ScenarioAssertions, ZoneAssertion};
    let mut s = basic();
    s.actions.clear();
    let mut object = s.initial_state.objects[1].clone();
    object.damage = 9;
    object.tapped = true;
    object.counters.insert(CounterKind::Shield, 1);
    object.counters.insert(CounterKind::PlusOnePlusOne, 4);
    s.initial_state.objects = vec![object];
    s.expected = ScenarioAssertions::default();
    s.expected.zones = vec![ZoneAssertion {
        zone: ZoneRef::of(Zone::Library, PlayerId(0)),
        card: Some(CardId(0)),
        owner: Some(PlayerId(0)),
        count: 1,
        counters: std::collections::BTreeMap::from([(CounterKind::Shield, 2)]),
    }];
    let report = minimize_with_report(&s, 32).unwrap();
    assert_eq!(
        report.reproduction.initial_state.objects.len(),
        1,
        "cannot substitute missing-object count failure for counter-total failure"
    );
    let object = &report.reproduction.initial_state.objects[0];
    assert_eq!(object.damage, 0);
    assert!(!object.tapped);
    assert!(!object.counters.contains_key(&CounterKind::PlusOnePlusOne));
    assert_eq!(
        report.reproduction.expected.zones[0].counters[&CounterKind::Shield],
        2
    );
    assert!(report.reductions >= 3);
    let failure = run(&report.reproduction, false).unwrap().0;
    assert!(!failure.pass);
    assert!(failure.message.contains("Shield counters 2"));
    assert!(report.reproduction.expected.final_digest.is_none());
}
