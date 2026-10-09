//! Independent adversarial review: expectations derive from state semantics,
//! rather than the canonical implementation's serialization structure.
mod support;

use mtg_core::{CardId, ObjectId, Timestamp, Zone, ZoneRef};
use mtg_engine::{Engine, GameObject, state::GameState, view::RevealedCard};
use mtg_verify::canonical::CanonicalGameState;
use support::*;

fn digest(state: GameState) -> String {
    CanonicalGameState::from_engine(&Engine::new(state))
        .unwrap()
        .digest()
}

fn state_with_objects(offset: u32, time_offset: u64) -> GameState {
    let mut state = main_state();
    for _ in 0..offset {
        state.new_object_id();
    }
    let mut library = Vec::new();
    for index in 0..2 {
        let id = state.new_object_id();
        let mut object = GameObject::new(id, CardId(index), P1, ZoneRef::of(Zone::Library, P1));
        object.timestamp = Timestamp(time_offset + index as u64 + 1);
        state.objects.insert(id, object);
        library.push(id);
    }
    state.generation = Timestamp(time_offset + 3);
    state
        .zone_order
        .insert(ZoneRef::of(Zone::Library, P1), library);
    state
}

#[test]
fn allocation_gaps_and_timestamp_offsets_do_not_change_observable_digest() {
    assert_eq!(
        digest(state_with_objects(0, 0)),
        digest(state_with_objects(100, 1000))
    );
}

#[test]
fn equal_timestamp_relation_is_distinct_from_ordered_timestamps() {
    let a = state_with_objects(0, 0);
    let mut b = a.clone();
    b.objects.get_mut(&ObjectId(2)).unwrap().timestamp = Timestamp(1);
    assert_ne!(digest(a), digest(b));
}

#[test]
fn ordered_library_is_not_normalized_into_a_set() {
    let a = state_with_objects(0, 0);
    let mut b = a.clone();
    b.zone_order
        .get_mut(&ZoneRef::of(Zone::Library, P1))
        .unwrap()
        .reverse();
    assert_ne!(digest(a), digest(b));
}

#[test]
fn map_insertion_order_does_not_change_digest() {
    let a = state_with_objects(0, 0);
    let mut b = a.clone();
    let objects: Vec<_> = b.objects.values().cloned().collect();
    b.objects.clear();
    for object in objects.into_iter().rev() {
        b.objects.insert(object.id, object);
    }
    assert_eq!(digest(a), digest(b));
}

#[test]
fn rng_progress_changes_digest_before_a_shuffle_occurs() {
    let a = main_state();
    let mut b = a.clone();
    b.rng.next_u64();
    assert_ne!(digest(a), digest(b));
}

#[test]
fn opponent_private_knowledge_changes_authoritative_digest() {
    let a = main_state();
    let mut b = a.clone();
    b.looked_at.push((
        P0,
        RevealedCard {
            owner: P1,
            card: CREATURE,
            face: 0,
        },
    ));
    assert_ne!(digest(a), digest(b));
}

#[test]
fn pending_priority_choice_changes_checkpoint() {
    let cards = Cards::creature(2, false);
    let mut engine = Engine::new(main_state());
    let before = CanonicalGameState::from_engine(&engine).unwrap();
    stabilize(&mut engine, &cards);
    let after = CanonicalGameState::from_engine(&engine).unwrap();
    assert_ne!(before.digest(), after.digest());
    assert!(!after.state["pending"].is_null());
}

#[test]
fn ownership_controller_and_life_are_distinct_digest_inputs() {
    let base = state_with_objects(0, 0);
    let original = digest(base.clone());
    let mut life = base.clone();
    life.players.get_mut(&P0).unwrap().life -= 1;
    assert_ne!(original, digest(life));
    let mut control = base.clone();
    control.objects.get_mut(&ObjectId(1)).unwrap().controller = P0;
    assert_ne!(original, digest(control));
    let mut ownership = base;
    ownership.objects.get_mut(&ObjectId(1)).unwrap().owner = P0;
    assert_ne!(original, digest(ownership));
}

#[test]
fn unscanned_event_history_changes_next_transition_and_digest() {
    use mtg_core::{Cause, Event, EventId, StampedEvent};
    let a = Engine::new(main_state());
    let mut b = Engine::new(main_state());
    b.log.push(StampedEvent {
        id: EventId(1),
        at: Timestamp(1),
        cause: Cause::PlayerAction(P1),
        event: Event::LifeChanged {
            player: P1,
            delta: -1,
        },
    });
    // Trigger detection consumes unscanned logs. Equal raw state is insufficient.
    assert_ne!(
        CanonicalGameState::from_engine(&a).unwrap().digest(),
        CanonicalGameState::from_engine(&b).unwrap().digest()
    );
}

fn scenario() -> mtg_verify::scenario::GameScenario {
    use mtg_verify::scenario::*;
    let mut fixture = GameScenario {
        advance_budget: mtg_verify::scenario::MAX_ADVANCE_BUDGET,
        format_version: FORMAT_VERSION,
        rules_version: RULES_VERSION.into(),
        engine_version: "independent-review".into(),
        card_db_hash: String::new(),
        seed: 17,
        metadata: Metadata::default(),
        cards: vec![Cards::creature(2, false).0],
        subtypes: vec![],
        initial_state: ScenarioState {
            players: vec![
                ScenarioPlayer {
                    id: P0,
                    life: 20,
                    poison: 0,
                    mana: Default::default(),
                },
                ScenarioPlayer {
                    id: P1,
                    life: 20,
                    poison: 0,
                    mana: Default::default(),
                },
            ],
            active_player: P0,
            turn: 2,
            step: mtg_core::Step::PrecombatMain,
            objects: vec![],
            commanders: Default::default(),
        },
        actions: vec![],
        expected: ScenarioAssertions::default(),
    };
    fixture.card_db_hash = fixture.card_hash();
    fixture
}

fn scenario_object(zone: ZoneRef) -> mtg_verify::scenario::ScenarioObject {
    mtg_verify::scenario::ScenarioObject {
        card: CREATURE,
        owner: P0,
        controller: None,
        zone,
        tapped: false,
        phased_out: false,
        damage: 0,
        counters: Default::default(),
    }
}

#[test]
fn command_zone_is_shared_in_scenario_validation() {
    let mut fixture = scenario();
    fixture
        .initial_state
        .objects
        .push(scenario_object(ZoneRef::shared(Zone::Command)));
    assert!(
        fixture.validate().is_ok(),
        "Command is a shared zone under CR 400.1"
    );
    fixture.initial_state.objects[0].zone = ZoneRef::of(Zone::Command, P0);
    assert!(
        fixture.validate().is_err(),
        "player-specific shared zone must be rejected"
    );
}

#[test]
fn scenarios_reject_objects_in_another_players_private_zone() {
    for zone in [Zone::Hand, Zone::Library, Zone::Graveyard] {
        let mut fixture = scenario();
        fixture
            .initial_state
            .objects
            .push(scenario_object(ZoneRef::of(zone, P1)));
        assert!(
            fixture.validate().is_err(),
            "CR 400.3 forbids ownership mismatch in {zone:?}"
        );
    }
}

#[test]
fn replay_reports_first_divergent_checkpoint_after_mutated_life() {
    use mtg_verify::scenario::{ScenarioAction, run};
    let mut fixture = scenario();
    fixture.actions.push(ScenarioAction {
        who: P0,
        answer: mtg_engine::Answer::Pass,
        expected_rejection: false,
        expected_choice: None,
        expected_state: None,
        expected_digest: None,
    });
    let (report, recorded) = run(&fixture, true).unwrap();
    assert!(report.pass);
    let mut changed = recorded;
    changed.initial_state.players[1].life -= 1;
    let (report, _) = run(&changed, false).unwrap();
    assert!(!report.pass);
    assert_eq!(report.first_divergent_action, Some(0));
    assert!(
        !report.diff.is_empty(),
        "divergence needs an actionable field diff"
    );
}

#[test]
fn eliminated_players_queued_extra_turns_do_not_begin() {
    use mtg_core::{PlayerId, Step};
    let p2 = PlayerId(2);
    let cards = Cards::creature(2, false);
    let mut state = GameState::new(&[P0, P1, p2], 20);
    state.turn = 2;
    state.step = Step::Cleanup;
    state.active_player = P0;
    state.players.get_mut(&p2).unwrap().has_lost = true;
    state.extra_turns.push(p2);
    let mut engine = Engine::new(state);
    stabilize(&mut engine, &cards);
    assert_eq!(
        engine.state.active_player, P1,
        "CR 800.4 skips eliminated players' scheduled turns"
    );
    assert!(!engine.state.player(engine.state.priority.unwrap()).has_lost);
}

#[test]
fn generation_clock_relation_to_existing_timestamps_changes_digest() {
    let distinct_clock = state_with_objects(0, 0);
    let mut coincident_clock = distinct_clock.clone();
    // Both clocks are at/after all object timestamps. Equality versus strict
    // order affects the relative rank of effects stamped on later transitions.
    coincident_clock.generation = Timestamp(2);
    assert_ne!(digest(distinct_clock), digest(coincident_clock));
}
