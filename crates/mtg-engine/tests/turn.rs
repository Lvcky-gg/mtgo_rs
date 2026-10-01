//! Turn structure: the step sequence, turn-based actions, and priority passing.

mod common;

use common::*;
use mtg_core::{Step, Zone, ZoneRef};
use mtg_engine::{Engine, state::GameState};

fn two_player_game() -> (Engine, TestCards) {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 1;
    state.active_player = P0;
    state.step = Step::Untap;
    state.priority = Some(P0);
    (Engine::new(state), TestCards::default())
}

#[test]
fn the_turn_runs_through_its_steps_and_then_passes_to_the_next_player() {
    let (mut engine, cards) = two_player_game();

    let seen = std::cell::RefCell::new(Vec::new());
    run(&mut engine, &cards, 4000, |e| {
        seen.borrow_mut().push(e.state.step);
        e.state.turn > 1
    });

    assert_eq!(engine.state.turn, 2, "the turn should have rolled over");
    assert_eq!(
        engine.state.active_player, P1,
        "the next player becomes active"
    );

    let seen = seen.into_inner();
    for step in [
        Step::Untap,
        Step::Upkeep,
        Step::Draw,
        Step::PrecombatMain,
        Step::DeclareAttackers,
        Step::CombatDamage,
        Step::PostcombatMain,
        Step::End,
        Step::Cleanup,
    ] {
        assert!(seen.contains(&step), "never reached {step:?}");
    }
}

#[test]
fn the_starting_player_skips_their_first_draw_step() {
    // CR 103.7a. The library is stocked, so a draw would be visible if it happened.
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 1;
    state.active_player = P0;
    state.priority = Some(P0);
    for _ in 0..5 {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Library, P0));
    }
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run(&mut engine, &cards, 2000, |e| {
        e.state.step == Step::PrecombatMain
    });

    let hand = engine.state.objects_in(ZoneRef::of(Zone::Hand, P0)).len();
    assert_eq!(
        hand, 0,
        "the starting player should not have drawn on turn one"
    );
}

#[test]
fn a_later_draw_step_does_draw() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);
    for _ in 0..5 {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Library, P0));
    }
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run(&mut engine, &cards, 2000, |e| {
        e.state.turn == 2 && e.state.step == Step::PrecombatMain
    });

    assert_eq!(
        engine.state.objects_in(ZoneRef::of(Zone::Hand, P0)).len(),
        1,
        "the active player draws for turn"
    );
    assert_eq!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Library, P0))
            .len(),
        4,
        "and the card came off the library"
    );
}

#[test]
fn drawing_from_an_empty_library_loses_the_game_but_only_via_a_state_based_action() {
    // CR 704.5b: the attempt is recorded, and the loss happens when state-based
    // actions are next checked — not inside the draw itself.
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run(&mut engine, &cards, 2000, |e| e.state.player(P0).has_lost);

    assert!(
        engine.state.player(P0).has_lost,
        "a player who drew from an empty library should have lost"
    );
}

#[test]
fn the_untap_step_untaps_only_the_active_players_permanents() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);
    for _ in 0..3 {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Library, P0));
    }
    let mine = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    let theirs = state.place(DUMMY, P1, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&mine).unwrap().tapped = true;
    state.objects.get_mut(&theirs).unwrap().tapped = true;

    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    run(&mut engine, &cards, 2000, |e| e.state.step == Step::Upkeep);

    assert!(
        !engine.state.objects[&mine].tapped,
        "the active player untaps"
    );
    assert!(engine.state.objects[&theirs].tapped, "other players do not");
}

#[test]
fn damage_wears_off_in_the_cleanup_step() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);
    for _ in 0..3 {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Library, P0));
    }
    // One damage on a 2/2 is not lethal, so it survives to see cleanup.
    let creature = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&creature).unwrap().damage = 1;

    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    run(&mut engine, &cards, 4000, |e| e.state.turn > 2);

    assert_eq!(
        engine.state.objects.get(&creature).map(|o| o.damage),
        Some(0),
        "marked damage should be removed during cleanup"
    );
}
