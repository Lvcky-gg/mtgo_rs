use mtg_core::PlayerId;
use mtg_verify::{
    campaign,
    scenario::{GameScenario, run},
};
use std::path::PathBuf;

fn basic() -> GameScenario {
    GameScenario::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/replays/basic_casting.json"),
    )
    .unwrap()
}

#[test]
fn identical_seed_campaign_records_identical_reproducible_artifacts() {
    let base = basic();
    let first = campaign::semantic(&base, 123456, 16).unwrap();
    let second = campaign::semantic(&base, 123456, 16).unwrap();
    assert!(first.failure.is_none(), "{:?}", first.failure);
    assert!(second.failure.is_none(), "{:?}", second.failure);
    assert_eq!(
        serde_json::to_value(&first.reproduction).unwrap(),
        serde_json::to_value(&second.reproduction).unwrap()
    );
    let (report, _) = run(&first.reproduction, false).unwrap();
    assert!(report.pass, "{}", report.message);
    assert_eq!(
        Some(report.digest),
        first.reproduction.expected.final_digest
    );
}

#[test]
fn minimizer_retains_failure_class_and_rejects_passing_input() {
    let base = basic();
    assert!(campaign::minimize(&base, 16).is_err());
    let mut failing = base;
    failing.expected.life.insert(PlayerId(0), 999);
    let (before, _) = run(&failing, false).unwrap();
    assert!(!before.pass);
    let reduced = campaign::minimize(&failing, 32).unwrap();
    let (after, _) = run(&reduced, false).unwrap();
    assert!(!after.pass);
    assert_eq!(before.message, after.message);
    assert!(reduced.actions.len() < failing.actions.len());
    // An assertion failure is intentionally simpler than an engine crash; this
    // does not claim minimization of state complexity or real-game failures.
}
