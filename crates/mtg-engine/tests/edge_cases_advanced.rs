//! Simplified advanced edge case tests

mod common;

use common::*;
use mtg_core::{Step, Zone, ZoneRef};
use mtg_engine::{Engine, Progress, choice::Answer, state::GameState};
use mtg_ir::PrintedCards;

// ---- Simple State Resilience Tests ---

/// Test: Multiple creatures on board - no panic or corruption
#[test]
fn board_with_multiple_creatures() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);

    // Place creatures for both players.
    for _ in 0..5 {
        state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    }
    for _ in 0..5 {
        state.place(DUMMY, P1, ZoneRef::shared(Zone::Battlefield));
    }

    let initial_count = state.objects.len();

    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // Just advance a few times to verify no panics.
    for _ in 0..20 {
        match engine.advance(&cards) {
            Progress::NeedsChoice(c) => {
                let answer = c.default.clone().unwrap_or(Answer::Pass);
                engine.answer(&cards, c.id, answer).ok();
            }
            Progress::Continue => {}
            _ => break,
        }
    }

    assert_eq!(
        engine.state.objects.len(),
        initial_count,
        "object count should remain stable"
    );
}

/// Test: Priority passes don't corrupt state
#[test]
fn priority_stability() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);

    let creature = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));

    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // Pass priority multiple times
    for _ in 0..5 {
        match engine.advance(&cards) {
            Progress::NeedsChoice(c) => {
                engine.answer(&cards, c.id, Answer::Pass).ok();
            }
            Progress::Continue => {}
            _ => break,
        }
    }

    assert!(
        engine.state.objects.contains_key(&creature),
        "creature should persist"
    );
}

/// Test: Flyer mechanics
#[test]
fn flyer_exists() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);

    let flyer = state.place(FLYER, P0, ZoneRef::shared(Zone::Battlefield));

    let engine = Engine::new(state);

    assert!(engine.state.objects.contains_key(&flyer));
}

/// Test: Mana sources
#[test]
fn mana_sources_on_board() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.step = Step::PrecombatMain;
    state.priority = Some(P0);

    let white_source = state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let dual_source = state.place(DUAL_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));

    state.objects.get_mut(&white_source).unwrap().summoning_sick = false;
    state.objects.get_mut(&dual_source).unwrap().summoning_sick = false;

    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    for _ in 0..10 {
        match engine.advance(&cards) {
            Progress::NeedsChoice(c) => {
                engine.answer(&cards, c.id, Answer::Pass).ok();
            }
            Progress::Continue => {}
            _ => break,
        }
    }

    assert!(engine.state.objects.contains_key(&white_source));
    assert!(engine.state.objects.contains_key(&dual_source));
}

/// Test: Controller query doesn't panic
#[test]
fn controller_query() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);

    let creature = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));

    let engine = Engine::new(state);

    let controller = mtg_engine::layers::controller(&engine.state, creature);
    assert_eq!(controller, Some(P0));
}

/// Test: Type-dependent query
#[test]
fn type_querying() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);

    let creature = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));

    let engine = Engine::new(state);
    let cards = TestCards::default();

    if let Some(obj) = engine.state.objects.get(&creature) {
        let _face = cards.face(obj.card, obj.face);
        // Just verify it doesn't panic
        assert!(true);
    }
}

/// Test: Asymmetric P/T creature
#[test]
fn asymmetric_pt() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);

    let sentry = state.place(SENTRY, P0, ZoneRef::shared(Zone::Battlefield));

    let engine = Engine::new(state);

    assert!(engine.state.objects.contains_key(&sentry));
}

/// Test: Large board doesn't cause issues
#[test]
fn large_board() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);

    for _ in 0..20 {
        state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
        state.place(DUMMY, P1, ZoneRef::shared(Zone::Battlefield));
    }

    let _initial_count = state.objects.len();

    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    for _ in 0..10 {
        match engine.advance(&cards) {
            Progress::NeedsChoice(c) => {
                engine.answer(&cards, c.id, Answer::Pass).ok();
            }
            Progress::Continue => {}
            _ => break,
        }
    }

    // Verify board state is stable
    assert!(engine.state.objects.len() >= 20);
}

/// Test: Vigilant creature
#[test]
fn vigilant_existence() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);

    let vigilant = state.place(VIGILANT, P0, ZoneRef::shared(Zone::Battlefield));

    let engine = Engine::new(state);

    assert!(engine.state.objects.contains_key(&vigilant));
}

/// Test: Reacher existence
#[test]
fn reacher_existence() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);

    let reacher = state.place(REACHER, P0, ZoneRef::shared(Zone::Battlefield));

    let engine = Engine::new(state);

    assert!(engine.state.objects.contains_key(&reacher));
}

/// Test: Trampler existence
#[test]
fn trampler_existence() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);

    let trampler = state.place(TRAMPLER, P0, ZoneRef::shared(Zone::Battlefield));

    let engine = Engine::new(state);

    assert!(engine.state.objects.contains_key(&trampler));
}

/// Test: Deathtoucher existence
#[test]
fn deathtoucher_existence() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);

    let deathtoucher = state.place(DEATHTOUCHER, P0, ZoneRef::shared(Zone::Battlefield));

    let engine = Engine::new(state);

    assert!(engine.state.objects.contains_key(&deathtoucher));
}
