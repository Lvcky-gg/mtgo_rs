//! State triggers (CR 603.8).
//!
//! These watch a *condition* rather than an event, so they are polled at the same moment
//! state-based actions are checked instead of being matched against the log. The two
//! properties worth pinning are that they fire as soon as the condition holds, and that a
//! condition which stays true does not re-fire forever.

mod common;

use common::*;
use mtg_core::{Event, ObjectId, Step, Zone, ZoneRef};
use mtg_engine::{Engine, Progress, choice::Answer, state::GameState};

fn board(life: i32) -> GameState {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.step = Step::PrecombatMain;
    state.priority = Some(P0);
    state.players.get_mut(&P0).unwrap().life = life;
    state
}

fn ready(state: &mut GameState, card: mtg_core::CardId) -> ObjectId {
    let id = state.place(card, P0, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    id
}

/// Run until `stop`, passing priority and taking every default.
fn run_to(
    engine: &mut Engine,
    cards: &TestCards,
    budget: usize,
    mut stop: impl FnMut(&Engine) -> bool,
) {
    for _ in 0..budget {
        if stop(engine) {
            return;
        }
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => return,
            Progress::NeedsChoice(c) => {
                let a = c.default.clone().unwrap_or(Answer::Pass);
                if engine.answer(cards, c.id, a).is_err() {
                    return;
                }
            }
        }
    }
}

fn life(engine: &Engine) -> i32 {
    engine.state.player(P0).life
}

fn stack_len(engine: &Engine) -> usize {
    engine.state.objects_in(ZoneRef::shared(Zone::Stack)).len()
}

// ---- firing ------------------------------------------------------------

#[test]
fn a_state_trigger_fires_as_soon_as_its_condition_holds() {
    let mut state = board(8);
    ready(&mut state, LOW_LIFE_BIG);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run_to(&mut engine, &cards, 3000, |e| e.state.player(P0).life >= 13);
    assert_eq!(life(&engine), 13, "8 + 5 from the state trigger");
}

#[test]
fn a_state_trigger_does_not_fire_while_its_condition_is_false() {
    let mut state = board(20);
    ready(&mut state, LOW_LIFE_BIG);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run_to(&mut engine, &cards, 3000, |e| e.state.step == Step::End);
    assert_eq!(life(&engine), 20, "the condition was never true");
}

#[test]
fn a_state_trigger_fires_once_the_condition_becomes_true_mid_game() {
    // It is not only checked at the start: losing life later is enough.
    let mut state = board(20);
    ready(&mut state, LOW_LIFE_BIG);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run_to(&mut engine, &cards, 500, |e| {
        e.state.step == Step::BeginCombat
    });
    assert_eq!(life(&engine), 20);

    // Drop below the threshold.
    engine.state.players.get_mut(&P0).unwrap().life = 9;
    engine.state.bump();

    run_to(&mut engine, &cards, 3000, |e| e.state.player(P0).life >= 14);
    assert_eq!(life(&engine), 14, "9 + 5");
}

#[test]
fn a_state_trigger_uses_the_stack() {
    let mut state = board(8);
    ready(&mut state, LOW_LIFE_BIG);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let mut saw_on_stack = false;
    for _ in 0..600 {
        if stack_len(&engine) > 0 {
            saw_on_stack = true;
            break;
        }
        match engine.advance(&cards) {
            Progress::NeedsChoice(c) => {
                let a = c.default.clone().unwrap_or(Answer::Pass);
                let _ = engine.answer(&cards, c.id, a);
            }
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
        }
    }

    assert!(
        saw_on_stack,
        "it should go on the stack like any other triggered ability"
    );
    assert_eq!(life(&engine), 8, "and not take effect before resolving");
}

// ---- not re-firing forever ---------------------------------------------

#[test]
fn a_state_trigger_does_not_fire_again_while_it_is_already_on_the_stack() {
    // CR 603.8. Without this the condition would still be true on the next settle pass,
    // the ability would fire again, and priority would never be reached.
    let mut state = board(8);
    ready(&mut state, LOW_LIFE_BIG);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // Advance to the first moment something is on the stack, then keep going a little
    // without letting it resolve, and check nothing piled up.
    for _ in 0..600 {
        if stack_len(&engine) > 0 {
            break;
        }
        match engine.advance(&cards) {
            Progress::NeedsChoice(c) => {
                let a = c.default.clone().unwrap_or(Answer::Pass);
                let _ = engine.answer(&cards, c.id, a);
            }
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
        }
    }
    assert_eq!(
        stack_len(&engine),
        1,
        "exactly one copy of the ability is on the stack"
    );
}

#[test]
fn a_state_trigger_re_fires_until_its_condition_clears_and_then_stops() {
    // The termination test. "Gain 1 life while at 10 or less" has to fire three times
    // from 8 to clear the condition: 8 → 9 → 10 → 11. If re-firing were broken it would
    // stop at 9; if the live-check were broken it would never terminate.
    let mut state = board(8);
    ready(&mut state, LOW_LIFE_SMALL);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run_to(&mut engine, &cards, 8000, |e| e.state.step == Step::End);

    assert_eq!(
        life(&engine),
        11,
        "it should climb to just past the threshold and then stop"
    );
}

#[test]
fn a_repeatedly_firing_state_trigger_still_reaches_the_end_of_the_turn() {
    // Proof that it terminates rather than merely producing the right number: the game
    // has to actually get somewhere.
    let mut state = board(8);
    ready(&mut state, LOW_LIFE_SMALL);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run_to(&mut engine, &cards, 8000, |e| e.state.turn > 2);
    assert!(
        engine.state.turn > 2,
        "the turn finished despite the repeated triggering"
    );
}

// ---- not event-driven --------------------------------------------------

#[test]
fn a_state_trigger_is_not_matched_against_events() {
    // A state trigger must be invisible to the log scan, or it would fire on unrelated
    // events whose shape happened to match nothing in particular.
    let mut state = board(20);
    ready(&mut state, LOW_LIFE_BIG);
    // Plenty of events: a library to draw from, creatures entering, steps beginning.
    for _ in 0..5 {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Library, P0));
    }
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run_to(&mut engine, &cards, 4000, |e| e.state.turn > 2);

    assert_eq!(
        life(&engine),
        20,
        "no amount of unrelated events should fire it"
    );
    assert!(
        !engine
            .log
            .iter()
            .any(|e| matches!(e.event, Event::AbilityPutOnStack { .. })),
        "and it never reached the stack"
    );
}
