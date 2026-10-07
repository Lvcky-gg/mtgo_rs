use mtg_verify::scenario::{GameScenario, run};
use std::{fs, path::PathBuf};

#[test]
fn every_replay_and_regression_fixture_matches_all_checkpoints() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut count = 0;
    for folder in ["tests/replays", "tests/regressions"] {
        let mut files: Vec<_> = fs::read_dir(root.join(folder))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect();
        files.sort();
        for path in files {
            let artifact = GameScenario::load(&path).unwrap();
            assert!(
                artifact.expected.final_digest.is_some(),
                "{} missing final digest",
                path.display()
            );
            assert!(
                artifact
                    .actions
                    .iter()
                    .all(|a| a.expected_digest.is_some() && a.expected_state.is_some()),
                "{} missing action checkpoints",
                path.display()
            );
            let (report, _) = run(&artifact, false).unwrap();
            assert!(
                report.pass,
                "{} diverged at {:?}: {}\n{:?}",
                path.display(),
                report.first_divergent_action,
                report.message,
                report.diff
            );
            count += 1;
        }
    }
    assert!(
        count >= 6,
        "initial replay corpus must not silently disappear"
    );
}

#[test]
fn altered_multiplayer_checkpoint_reports_first_divergent_action() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/replays/multiplayer_priority.json");
    let mut artifact = GameScenario::load(&path).unwrap();
    let index = 2;
    artifact.actions[index].expected_digest = Some("deliberately altered checkpoint".into());
    let (report, _) = run(&artifact, false).unwrap();
    assert!(!report.pass);
    assert_eq!(report.first_divergent_action, Some(index));
}

#[test]
fn altered_multiplayer_life_reports_first_actual_state_divergence() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/replays/multiplayer_priority.json");
    let mut artifact = GameScenario::load(&path).unwrap();
    artifact.initial_state.players[0].life += 1;
    let (report, _) = run(&artifact, false).unwrap();
    assert!(!report.pass);
    assert_eq!(report.first_divergent_action, Some(0));
    assert!(
        !report.diff.is_empty(),
        "actual state difference must be explained"
    );
}

#[test]
fn regression_canonical_life_omission_cannot_hide_changed_life() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/regressions/issue_local_canonical_life_omission.json");
    let mut artifact = GameScenario::load(&path).unwrap();
    // No explicit life assertion: this tests snapshot completeness independently
    // of the runner's separately implemented expected-life assertions.
    assert!(artifact.expected.life.is_empty());
    artifact.initial_state.players[0].life += 1;
    let (report, _) = run(&artifact, false).unwrap();
    assert!(!report.pass);
    assert_eq!(report.first_divergent_action, Some(0));
    assert!(!report.diff.is_empty());
}

#[test]
fn regression_priority_after_elimination_belongs_to_next_survivor() {
    use mtg_core::PlayerId;
    use mtg_verify::scenario::next_choice;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/regressions/issue_local_priority_eliminated_player.json");
    let artifact = GameScenario::load(&path).unwrap();
    let (mut engine, cards) = artifact.setup().unwrap();
    for action in &artifact.actions {
        let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
        assert_eq!(choice.who, action.who);
        engine
            .answer(&cards, choice.id, action.answer.clone())
            .unwrap();
    }
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    assert_eq!(choice.who, PlayerId(0));
    assert_eq!(engine.state.priority, Some(PlayerId(0)));
    assert!(engine.state.players[&PlayerId(3)].has_lost);
}
