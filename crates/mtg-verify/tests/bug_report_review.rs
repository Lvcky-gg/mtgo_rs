//! Independent intake contract: captured outcomes are evidence, never replacement oracles.
use mtg_core::PlayerId;
use mtg_verify::{
    bug_report::{MAX_REPORT_BYTES, ReplayReport, capture},
    scenario::GameScenario,
};
use std::path::PathBuf;
fn basic() -> GameScenario {
    GameScenario::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/replays/basic_casting.json"),
    )
    .unwrap()
}
#[test]
fn passing_and_failing_reports_round_trip_without_blessing_assertions() {
    for failing in [false, true] {
        let mut s = basic();
        if failing {
            s.expected.life.insert(PlayerId(0), 999);
        }
        s.metadata.issue = Some("private-intake-example".into());
        let before = serde_json::to_value(&s).unwrap();
        let report = capture(&s).unwrap();
        assert_eq!(serde_json::to_value(&report.scenario).unwrap(), before);
        assert_eq!(report.observed.pass, !failing);
        let parsed = ReplayReport::parse(&serde_json::to_vec(&report).unwrap()).unwrap();
        assert_eq!(
            parsed.verify().unwrap().pass,
            !failing,
            "reproducing a failed outcome is valid evidence, never a passing rules oracle"
        );
    }
}
#[test]
fn tampering_with_any_material_observation_is_rejected() {
    let original = capture(&basic()).unwrap();
    for change in 0..5 {
        let mut report = original.clone();
        match change {
            0 => report.observed.pass = false,
            1 => report.observed.digest = "forged-digest".into(),
            2 => report.observed.first_divergent_action = Some(0),
            3 => report.observed.message.push_str(" forged"),
            _ => report.observed.diff.push("forged-state-difference".into()),
        }
        assert!(report.verify().is_err());
    }
}
#[test]
fn changed_scenario_transition_cannot_match_old_observation() {
    let mut report = capture(&basic()).unwrap();
    report.scenario.initial_state.players[0].life += 1;
    assert!(report.verify().is_err());
}
#[test]
fn report_parser_rejects_unknown_format_unknown_fields_and_bad_nested_scenario() {
    let report = capture(&basic()).unwrap();
    let mut json = serde_json::to_value(&report).unwrap();
    json["format_version"] = 999.into();
    assert!(ReplayReport::parse(&serde_json::to_vec(&json).unwrap()).is_err());
    json = serde_json::to_value(&report).unwrap();
    json["trusted"] = true.into();
    assert!(ReplayReport::parse(&serde_json::to_vec(&json).unwrap()).is_err());
    json = serde_json::to_value(&report).unwrap();
    json["scenario"]["card_db_hash"] = "forged".into();
    assert!(ReplayReport::parse(&serde_json::to_vec(&json).unwrap()).is_err());
}
#[test]
fn report_parser_enforces_outer_size_limit_before_deserialization() {
    assert!(ReplayReport::parse(&vec![b' '; MAX_REPORT_BYTES + 1]).is_err());
}
#[test]
fn directly_constructed_unsupported_report_cannot_bypass_validation() {
    let mut report = capture(&basic()).unwrap();
    report.format_version = 999;
    assert!(report.verify().is_err());
}

#[test]
fn cli_exports_failed_evidence_reproduces_it_and_never_overwrites() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let dir = std::env::temp_dir().join(format!(
        "mtg-report-review-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    let input = dir.join("scenario.json");
    let output = dir.join("report.json");
    let mut scenario = basic();
    scenario.expected.life.insert(PlayerId(0), 999);
    let original = serde_json::to_vec_pretty(&scenario).unwrap();
    fs::write(&input, &original).unwrap();
    let cli = env!("CARGO_BIN_EXE_mtgo-rs");
    let export = Command::new(cli)
        .arg("scenario")
        .arg("report")
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(export.status.code(), Some(1));
    let saved = fs::read(&output).expect("failed replay must still export bug evidence");
    let report = ReplayReport::parse(&saved).unwrap();
    assert!(!report.observed.pass);
    assert_eq!(report.scenario.expected.life[&PlayerId(0)], 999);
    assert_eq!(fs::read(&input).unwrap(), original, "input remains intact");
    let replay = Command::new(cli)
        .arg("report")
        .arg("replay")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&replay.stdout).unwrap();
    assert_eq!(response["reproduced"], true);
    assert_eq!(response["observed"]["pass"], false);
    let overwrite = Command::new(cli)
        .arg("scenario")
        .arg("report")
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(overwrite.status.code(), Some(1));
    assert_eq!(
        fs::read(&output).unwrap(),
        saved,
        "existing attachment must not be overwritten"
    );
    let mut tampered = report;
    tampered.observed.digest = "forged-observation".into();
    let tampered_path = dir.join("tampered.json");
    fs::write(&tampered_path, serde_json::to_vec(&tampered).unwrap()).unwrap();
    let replay = Command::new(cli)
        .arg("report")
        .arg("replay")
        .arg(&tampered_path)
        .output()
        .unwrap();
    assert_eq!(replay.status.code(), Some(1));
    assert_eq!(fs::read(&output).unwrap(), saved);
    fs::remove_dir_all(&dir).unwrap();
}
