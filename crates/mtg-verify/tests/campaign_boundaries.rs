//! Independent campaign boundary and emitted-artifact reproducibility checks.
use mtg_core::PlayerId;
use mtg_verify::{
    campaign, invariants,
    scenario::{GameScenario, next_choice, run},
};
use std::path::PathBuf;
fn basic() -> GameScenario {
    GameScenario::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/replays/basic_casting.json"),
    )
    .unwrap()
}
#[test]
fn zero_action_campaign_emits_a_replayable_choice_boundary() {
    let report = campaign::semantic(&basic(), 901, 0).unwrap();
    assert_eq!(report.actions, 0);
    assert!(report.failure.is_none());
    let replay = run(&report.reproduction, false).unwrap().0;
    assert!(
        replay.pass,
        "zero-action successful artifact must reproduce: {} {:?}",
        replay.message, replay.diff
    );
}
#[test]
fn terminal_initial_state_with_no_actions_is_reproducible_and_structurally_valid() {
    let mut base = basic();
    base.actions.clear();
    base.expected = Default::default();
    base.initial_state.players[1].life = 0;
    let (mut engine, cards) = base.setup().unwrap();
    assert!(next_choice(&mut engine, &cards).unwrap().is_none());
    invariants::verify(&engine.state, &cards).unwrap();
    for steps in [0, 1, 8] {
        let report = campaign::semantic(&base, 902, steps).unwrap();
        assert!(report.failure.is_none());
        assert_eq!(report.actions, 0);
        assert!(
            run(&report.reproduction, false).unwrap().0.pass,
            "terminal corpus artifacts must match replay normalization"
        );
    }
}
#[test]
fn invalid_initial_negative_counter_is_rejected_before_any_campaign_actions() {
    let mut base = basic();
    base.initial_state.objects[0]
        .counters
        .insert(mtg_core::CounterKind::Shield, -1);
    assert!(campaign::semantic(&base, 903, 0).is_err());
    assert!(run(&base, false).is_err());
}
#[test]
fn legal_concession_terminal_replay_is_structurally_valid() {
    let mut base = basic();
    base.actions.clear();
    base.expected = Default::default();
    base.actions.push(mtg_verify::scenario::ScenarioAction {
        who: PlayerId(0),
        answer: mtg_engine::Answer::Action(mtg_engine::Action::Concede),
        expected_rejection: false,
        expected_choice: None,
        expected_state: None,
        expected_digest: None,
    });
    let (mut engine, cards) = base.setup().unwrap();
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    engine
        .answer(&cards, choice.id, base.actions[0].answer.clone())
        .unwrap();
    assert!(next_choice(&mut engine, &cards).unwrap().is_none());
    assert_eq!(
        engine.state.priority, None,
        "CR104.1: the game ends without further priority"
    );
    assert!(engine.state.player(PlayerId(0)).has_lost);
    match engine.advance(&cards) {
        mtg_engine::Progress::GameOver { winners } => assert_eq!(winners, vec![PlayerId(1)]),
        other => panic!("remaining player must win: {other:?}"),
    }
    invariants::verify(&engine.state, &cards).unwrap();
    let (report, artifact) = run(&base, true).unwrap();
    assert!(report.pass, "{} {:?}", report.message, report.diff);
    assert!(run(&artifact, false).unwrap().0.pass);
}
