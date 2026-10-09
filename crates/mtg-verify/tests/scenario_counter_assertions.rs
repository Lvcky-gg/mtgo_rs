//! Independent checks that checkpoint recording cannot bless incorrect counter totals.
use mtg_core::{CardId, CounterKind, PlayerId, Zone, ZoneRef};
use mtg_verify::scenario::{GameScenario, ScenarioAssertions, ScenarioObject, ZoneAssertion, run};
use std::{collections::BTreeMap, path::PathBuf};
fn scenario() -> GameScenario {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/replays/multiplayer_priority.json");
    let mut s = GameScenario::load(&path).unwrap();
    s.actions.clear();
    s.initial_state.objects.clear();
    s.expected = ScenarioAssertions::default();
    s.cards.push(s.cards[0].clone());
    s.card_db_hash = s.card_hash();
    s
}
fn object(card: u32, owner: u8, zone: Zone, shields: Option<i32>) -> ScenarioObject {
    ScenarioObject {
        card: CardId(card),
        owner: PlayerId(owner),
        controller: None,
        zone: if zone.is_shared() {
            ZoneRef::shared(zone)
        } else {
            ZoneRef::of(zone, PlayerId(owner))
        },
        tapped: false,
        phased_out: false,
        damage: 0,
        counters: shields
            .map(|n| BTreeMap::from([(CounterKind::Shield, n)]))
            .unwrap_or_default(),
    }
}
fn assertion(count: usize, shields: i32) -> ZoneAssertion {
    ZoneAssertion {
        zone: ZoneRef::shared(Zone::Battlefield),
        card: Some(CardId(0)),
        owner: Some(PlayerId(0)),
        count,
        counters: BTreeMap::from([(CounterKind::Shield, shields)]),
    }
}
#[test]
fn recording_and_running_both_reject_wrong_counter_total() {
    let mut s = scenario();
    s.initial_state
        .objects
        .push(object(0, 0, Zone::Battlefield, Some(2)));
    s.expected.zones.push(assertion(1, 1));
    for record in [false, true] {
        let (report, result) = run(&s, record).unwrap();
        assert!(!report.pass);
        assert!(report.message.contains("actual 2"));
        assert_eq!(result.expected.zones[0].counters[&CounterKind::Shield], 1);
        assert!(result.expected.final_digest.is_none());
    }
}
#[test]
fn counter_totals_obey_zone_card_owner_filters_and_include_missing_as_zero() {
    let mut s = scenario();
    s.initial_state.objects = vec![
        object(0, 0, Zone::Battlefield, Some(2)),
        object(0, 0, Zone::Battlefield, Some(3)),
        object(0, 0, Zone::Battlefield, None),
        object(0, 1, Zone::Battlefield, Some(7)),
        object(1, 0, Zone::Battlefield, Some(11)),
        object(0, 0, Zone::Graveyard, Some(13)),
    ];
    let mut expected = assertion(3, 5);
    expected.counters.insert(CounterKind::PlusOnePlusOne, 0);
    s.expected.zones.push(expected);
    for record in [false, true] {
        assert!(run(&s, record).unwrap().0.pass);
    }
}
#[test]
fn absent_counters_are_zero_even_for_an_empty_selection() {
    let mut s = scenario();
    s.expected.zones.push(assertion(0, 0));
    assert!(run(&s, true).unwrap().0.pass);
    s.expected.zones[0].counters.insert(CounterKind::Shield, 1);
    assert!(!run(&s, true).unwrap().0.pass);
}
#[test]
fn totals_larger_than_i32_are_compared_without_overflow_or_wraparound() {
    let mut s = scenario();
    s.initial_state.objects = vec![
        object(0, 0, Zone::Battlefield, Some(i32::MAX)),
        object(0, 0, Zone::Battlefield, Some(i32::MAX)),
    ];
    s.expected.zones.push(assertion(2, -2));
    for record in [false, true] {
        let (report, _) = run(&s, record).unwrap();
        assert!(!report.pass);
        assert!(report.message.contains("actual 4294967294"));
    }
}
