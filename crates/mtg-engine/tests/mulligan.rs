//! The London mulligan (CR 103.5): keep, or shuffle and draw seven again, then bottom one card
//! per mulligan.

mod common;

use common::*;
use mtg_core::{Step, Zone, ZoneRef};
use mtg_engine::{
    Choice, ChoiceKind, Engine, Progress,
    choice::Answer,
    state::{GameState, Pregame, Rng},
};

fn game() -> Engine {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 1;
    state.active_player = P0;
    state.step = Step::Untap;
    state.priority = Some(P0);
    for p in [P0, P1] {
        for i in 0..40 {
            let card = if i % 2 == 0 { DUMMY } else { FIELD };
            let zone = if i < 7 {
                ZoneRef::of(Zone::Hand, p)
            } else {
                ZoneRef::of(Zone::Library, p)
            };
            state.place(card, p, zone);
        }
    }
    state.pregame = Some(Pregame::new(vec![P0, P1]));
    state.rng = Rng::from_seed(&[7; 32]);
    Engine::new(state)
}

fn next(engine: &mut Engine) -> Choice {
    let cards = TestCards::default();
    loop {
        match engine.advance(&cards) {
            Progress::NeedsChoice(c) => return c,
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game over"),
        }
    }
}

fn answer(engine: &mut Engine, c: &Choice, a: Answer) {
    engine.answer(&TestCards::default(), c.id, a).unwrap();
}

fn hand(engine: &Engine, p: mtg_core::PlayerId) -> Vec<mtg_core::ObjectId> {
    engine.state.objects_in(ZoneRef::of(Zone::Hand, p))
}

#[test]
fn the_starting_player_decides_first_and_keeping_starts_the_game() {
    let mut engine = game();
    let c = next(&mut engine);
    assert_eq!(c.who, P0);
    assert!(matches!(
        c.kind,
        ChoiceKind::KeepOrMulligan { mulligans_taken: 0 }
    ));
    answer(&mut engine, &c, Answer::Bool(true));

    let c = next(&mut engine);
    assert_eq!(c.who, P1, "then the next player");
    answer(&mut engine, &c, Answer::Bool(true));

    let c = next(&mut engine);
    assert!(
        matches!(c.kind, ChoiceKind::Priority { .. }),
        "then the game: {:?}",
        c.kind
    );
    assert!(engine.state.pregame.is_none());
}

#[test]
fn a_mulligan_deals_a_fresh_seven_and_keeping_bottoms_one() {
    let mut engine = game();
    let before = hand(&engine, P0);
    let c = next(&mut engine);
    answer(&mut engine, &c, Answer::Bool(false));

    let after = hand(&engine, P0);
    assert_eq!(after.len(), 7, "a fresh seven");
    assert_eq!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Library, P0))
            .len(),
        33
    );
    assert_ne!(before, after, "a different hand");

    let c = next(&mut engine);
    assert!(matches!(
        c.kind,
        ChoiceKind::KeepOrMulligan { mulligans_taken: 1 }
    ));
    answer(&mut engine, &c, Answer::Bool(true));

    let c = next(&mut engine);
    let ChoiceKind::ChooseObjects {
        from,
        min: 1,
        max: 1,
    } = &c.kind
    else {
        panic!("{:?}", c.kind)
    };
    let bottomed = from[3];
    answer(&mut engine, &c, Answer::Objects(vec![bottomed]));

    assert_eq!(
        hand(&engine, P0).len(),
        6,
        "keeping after one mulligan means six"
    );
    let library = engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
    assert_eq!(library.last(), Some(&bottomed), "on the bottom");
}

#[test]
fn bottoming_the_wrong_number_is_refused_and_asked_again() {
    let mut engine = game();
    let c = next(&mut engine);
    answer(&mut engine, &c, Answer::Bool(false));
    let c = next(&mut engine);
    answer(&mut engine, &c, Answer::Bool(true));
    let c = next(&mut engine);
    let ChoiceKind::ChooseObjects { from, .. } = &c.kind else {
        panic!()
    };
    let two = vec![from[0], from[1]];
    assert!(
        engine
            .answer(&TestCards::default(), c.id, Answer::Objects(two))
            .is_err()
    );
    assert_eq!(next(&mut engine).id, c.id, "the same question stands");
}

#[test]
fn the_same_seed_makes_the_same_mulligan() {
    let deal = || {
        let mut engine = game();
        let c = next(&mut engine);
        answer(&mut engine, &c, Answer::Bool(false));
        hand(&engine, P0)
    };
    assert_eq!(deal(), deal(), "deterministic: a replay reproduces it");
}
