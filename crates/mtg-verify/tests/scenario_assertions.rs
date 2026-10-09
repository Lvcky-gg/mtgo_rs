use mtg_core::{CardId, PlayerId, Zone, ZoneRef};
use mtg_verify::scenario::{GameScenario, ZoneAssertion, run};
use std::path::PathBuf;

fn basic() -> GameScenario {
    GameScenario::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/replays/basic_casting.json"),
    )
    .unwrap()
}
fn spell_graveyard(count: usize) -> ZoneAssertion {
    ZoneAssertion {
        zone: ZoneRef::of(Zone::Graveyard, PlayerId(0)),
        card: Some(CardId(1)),
        owner: Some(PlayerId(0)),
        counters: Default::default(),
        count,
    }
}

#[test]
fn independently_expected_zone_counts_pass_in_replay_and_record_modes() {
    let mut fixture = basic();
    fixture.expected.zones = vec![
        spell_graveyard(1),
        ZoneAssertion {
            zone: ZoneRef::of(Zone::Hand, PlayerId(0)),
            card: Some(CardId(1)),
            owner: Some(PlayerId(0)),
            counters: Default::default(),
            count: 0,
        },
    ];
    for record in [false, true] {
        let (report, _) = run(&fixture, record).unwrap();
        assert!(report.pass, "{}", report.message);
    }
}

#[test]
fn incorrect_human_zone_expectation_is_never_blessed_by_recording() {
    let mut fixture = basic();
    fixture.expected.zones = vec![spell_graveyard(2)];
    for record in [false, true] {
        let (report, _) = run(&fixture, record).unwrap();
        assert!(
            !report.pass,
            "record mode must not overwrite an independent expected count"
        );
        assert!(report.message.contains("zone") || report.message.contains("Zone"));
    }
}

#[test]
fn malformed_zone_assertion_references_are_rejected_before_execution() {
    let mut fixture = basic();
    let mut malformed = vec![];
    let mut assertion = spell_graveyard(1);
    assertion.card = Some(CardId(9999));
    malformed.push(assertion);
    let mut assertion = spell_graveyard(1);
    assertion.owner = Some(PlayerId(7));
    malformed.push(assertion);
    let mut assertion = spell_graveyard(1);
    assertion.zone = ZoneRef::of(Zone::Battlefield, PlayerId(0));
    malformed.push(assertion);
    let mut assertion = spell_graveyard(1);
    assertion.zone = ZoneRef::shared(Zone::Hand);
    malformed.push(assertion);
    for assertion in malformed {
        fixture.expected.zones = vec![assertion];
        assert!(fixture.validate().is_err());
        assert!(run(&fixture, true).is_err());
    }
}
