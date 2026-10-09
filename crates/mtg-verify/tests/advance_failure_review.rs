//! Independent budget exhaustion evidence. A tiny budget deliberately interrupts
//! legal automatic transitions; this does not claim an engine infinite loop.
use mtg_verify::{
    bug_report::{ReplayReport, capture},
    campaign::{self, minimize_with_report},
    scenario::{GameScenario, run},
};
use std::path::PathBuf;
fn basic() -> GameScenario {
    let mut s = GameScenario::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/replays/basic_casting.json"),
    )
    .unwrap();
    for action in &mut s.actions {
        action.expected_choice = None;
        action.expected_digest = None;
        action.expected_state = None;
    }
    s.expected = Default::default();
    s
}
fn budget(mut scenario: GameScenario, amount: usize) -> GameScenario {
    // Exercise the public human-readable artifact contract, including validation.
    let mut value = serde_json::to_value(&scenario).unwrap();
    value["advance_budget"] = amount.into();
    scenario = GameScenario::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
    scenario
}
#[test]
fn initial_exhaustion_is_structured_replayable_evidence_without_action_index() {
    let s = GameScenario::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/failures/advance_budget_exhaustion.json"),
    )
    .unwrap();
    for record in [false, true] {
        let (report, artifact) =
            run(&s, record).expect("automatic exhaustion is FAIL evidence, not parser error");
        assert!(!report.pass);
        assert!(report.message.contains("advance budget exceeded"));
        assert_eq!(report.first_divergent_action, None);
        assert!(artifact.expected.final_digest.is_none());
        assert_eq!(
            serde_json::to_value(artifact).unwrap(),
            serde_json::to_value(&s).unwrap()
        );
    }
    let attachment = capture(&s).unwrap();
    assert!(!attachment.verify().unwrap().pass);
    let parsed = ReplayReport::parse(&serde_json::to_vec(&attachment).unwrap()).unwrap();
    assert_eq!(parsed.verify().unwrap(), attachment.observed);
}
#[test]
fn after_action_exhaustion_preserves_exact_action_and_canonical_observation() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/replays/enters_trigger.json");
    let mut base = GameScenario::load(&path).unwrap();
    for action in &mut base.actions {
        action.expected_choice = None;
        action.expected_digest = None;
        action.expected_state = None;
    }
    base.expected = Default::default();
    // Independently measured: initial priority fits budget4; action2's
    // creature resolution plus enters-trigger boundary requires more advances.
    let s = budget(base, 4);
    let before = run(&s, false).unwrap().0;
    assert!(!before.pass);
    assert!(before.message.contains("advance budget exceeded"));
    assert_eq!(before.first_divergent_action, Some(2));
    let attachment = capture(&s).unwrap();
    assert_eq!(attachment.observed, before);
    assert_eq!(attachment.verify().unwrap(), before);
    let (recorded, _) = run(&s, true).unwrap();
    assert!(!recorded.pass);
    assert_eq!(
        recorded.first_divergent_action,
        before.first_divergent_action
    );
    let minimized = minimize_with_report(&s, 32).unwrap();
    let after = run(&minimized.reproduction, false).unwrap().0;
    assert!(!after.pass);
    assert_eq!(after.message, before.message);
    assert!(after.first_divergent_action.is_some());
    assert_eq!(
        serde_json::to_value(&minimized.reproduction).unwrap()["advance_budget"],
        serde_json::to_value(&s).unwrap()["advance_budget"]
    );
}
#[test]
fn semantic_failure_artifact_and_observed_report_match_replay() {
    let s = budget(basic(), 1);
    let campaign = campaign::semantic(&s, 481, 8).unwrap();
    assert!(campaign.failure.is_some());
    assert_eq!(campaign.actions, 0);
    let replay = run(&campaign.reproduction, false).unwrap().0;
    assert!(!replay.pass);
    let value = serde_json::to_value(&campaign).unwrap();
    assert_eq!(value["observed"], serde_json::to_value(&replay).unwrap());
    assert_eq!(campaign.failure.as_deref(), Some(replay.message.as_str()));
    let reduced = minimize_with_report(&campaign.reproduction, 8).unwrap();
    assert!(
        !capture(&reduced.reproduction)
            .unwrap()
            .verify()
            .unwrap()
            .pass
    );
}
#[test]
fn zero_and_excessive_advance_budgets_are_rejected() {
    for amount in [0, 10001] {
        let mut value = serde_json::to_value(basic()).unwrap();
        value["advance_budget"] = amount.into();
        assert!(GameScenario::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}

#[test]
fn successful_campaign_observed_report_exactly_matches_replay() {
    for steps in [0, 1, 8] {
        let report = campaign::semantic(&basic(), 481, steps).unwrap();
        assert!(report.failure.is_none());
        assert_eq!(
            report.observed.as_ref().unwrap(),
            &run(&report.reproduction, false).unwrap().0
        );
    }
}

#[test]
fn legacy_artifact_defaults_to_full_budget_and_omits_default_on_export() {
    let s = basic();
    assert_eq!(s.advance_budget, 10000);
    let value = serde_json::to_value(&s).unwrap();
    assert!(value.get("advance_budget").is_none());
    assert_eq!(
        GameScenario::parse(&serde_json::to_vec(&value).unwrap())
            .unwrap()
            .advance_budget,
        10000
    );
    assert_eq!(
        serde_json::to_value(budget(s, 1)).unwrap()["advance_budget"],
        1
    );
}

#[test]
fn cli_campaign_exhaustion_exports_scenario_observation_and_replay_attachment() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/failures/advance_budget_exhaustion.json");
    let dir = std::env::temp_dir().join(format!(
        "mtg-advance-cli-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    let scenario_path = dir.join("failure.json");
    let attachment_path = dir.join("attachment.json");
    let cli = env!("CARGO_BIN_EXE_mtgo-rs");
    let generated = Command::new(cli)
        .args(["scenario", "fuzz"])
        .arg(&fixture)
        .arg(&scenario_path)
        .env("MTGO_VERIFY_ACTIONS", "8")
        .output()
        .unwrap();
    assert_eq!(generated.status.code(), Some(1));
    let scenario =
        GameScenario::load(&scenario_path).expect("failed campaign must emit reproducible input");
    let actual = run(&scenario, false).unwrap().0;
    assert!(!actual.pass);
    assert!(actual.message.contains("advance budget exceeded"));
    let stdout = String::from_utf8(generated.stdout).unwrap();
    let json_start = stdout
        .find('{')
        .expect("campaign stdout contains structured observed state");
    let observed: serde_json::Value = serde_json::from_str(&stdout[json_start..]).unwrap();
    assert_eq!(observed, serde_json::to_value(&actual).unwrap());
    assert!(!actual.digest.is_empty());
    let exported = Command::new(cli)
        .args(["scenario", "report"])
        .arg(&scenario_path)
        .arg(&attachment_path)
        .output()
        .unwrap();
    assert_eq!(exported.status.code(), Some(1));
    let attachment = ReplayReport::load(&attachment_path).unwrap();
    assert_eq!(attachment.observed, actual);
    let replayed = Command::new(cli)
        .args(["report", "replay"])
        .arg(&attachment_path)
        .output()
        .unwrap();
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&replayed.stdout).unwrap();
    assert_eq!(response["reproduced"], true);
    assert_eq!(response["observed"], observed);
    fs::remove_dir_all(dir).unwrap();
}
