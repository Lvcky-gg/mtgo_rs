use mtg_core::{ObjectId, Target};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};
use mtg_verify::{
    canonical::CanonicalGameState,
    scenario::{GameScenario, next_choice, run},
};

fn fixture() -> GameScenario {
    GameScenario::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/regressions/issue_local_malformed_target_rejection.json"),
    )
    .unwrap()
}

#[test]
fn rejected_target_replay_keeps_prompt_until_a_valid_answer() {
    let scenario = fixture();
    assert_eq!(
        scenario
            .actions
            .iter()
            .filter(|action| action.expected_rejection)
            .count(),
        6
    );
    let (report, _) = run(&scenario, false).unwrap();
    assert!(
        report.pass,
        "{} at {:?}",
        report.message, report.first_divergent_action
    );
}

#[test]
fn recording_cannot_bless_an_accepted_answer_marked_for_rejection() {
    let mut scenario = fixture();
    scenario.actions.truncate(1);
    scenario.actions[0].answer = Answer::Pass;
    scenario.actions[0].expected_choice = None;
    scenario.actions[0].expected_state = None;
    scenario.actions[0].expected_digest = None;
    scenario.actions[0].expected_rejection = true;
    for record in [false, true] {
        let (report, _) = run(&scenario, record).unwrap();
        assert!(!report.pass);
        assert_eq!(report.first_divergent_action, Some(0));
        assert!(report.message.contains("answer was accepted"));
    }
}

#[test]
fn an_unexpected_rejection_is_still_a_replay_failure() {
    let mut scenario = fixture();
    scenario.actions.truncate(2);
    scenario.actions[1].expected_rejection = false;
    let (report, _) = run(&scenario, false).unwrap();
    assert!(!report.pass);
    assert_eq!(report.first_divergent_action, Some(1));
    assert!(report.message.contains("Illegal recorded answer"));
}

#[test]
fn malformed_targets_preserve_full_engine_state_and_choice_identity() {
    let scenario = fixture();
    let (mut engine, cards) = scenario.setup().unwrap();
    let priority = next_choice(&mut engine, &cards).unwrap().unwrap();
    engine
        .answer(
            &cards,
            priority.id,
            Answer::Action(Action::Cast {
                object: ObjectId(2),
            }),
        )
        .unwrap();
    let targets = next_choice(&mut engine, &cards).unwrap().unwrap();
    assert!(matches!(targets.kind, ChoiceKind::ChooseTargets { .. }));
    let before = CanonicalGameState::from_engine(&engine).unwrap();
    for action in scenario
        .actions
        .iter()
        .filter(|action| action.expected_rejection)
    {
        assert!(
            engine
                .answer(&cards, targets.id, action.answer.clone())
                .is_err()
        );
        assert_eq!(CanonicalGameState::from_engine(&engine).unwrap(), before);
        let again = next_choice(&mut engine, &cards).unwrap().unwrap();
        assert_eq!(again.id, targets.id);
        assert_eq!(
            serde_json::to_value(&again.kind).unwrap(),
            serde_json::to_value(&targets.kind).unwrap()
        );
    }
    engine
        .answer(
            &cards,
            targets.id,
            Answer::Targets(vec![vec![Target::Object(ObjectId(1))]]),
        )
        .unwrap();
    assert!(next_choice(&mut engine, &cards).unwrap().is_some());
}

#[test]
fn rejection_recording_replays_and_records_unchanged_checkpoints() {
    let scenario = fixture();
    let (recorded_report, recorded) = run(&scenario, true).unwrap();
    assert!(recorded_report.pass);
    let first_rejection = recorded.actions[1].expected_digest.as_ref().unwrap();
    for action in &recorded.actions[1..7] {
        assert!(action.expected_rejection);
        assert_eq!(action.expected_digest.as_ref().unwrap(), first_rejection);
    }
    let (replayed, _) = run(&recorded, false).unwrap();
    assert!(replayed.pass, "{}", replayed.message);
    assert_eq!(replayed.digest, recorded_report.digest);
}
