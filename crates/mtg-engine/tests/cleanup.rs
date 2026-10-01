//! CR 514.1: at cleanup the active player discards down to seven.

mod common;

use common::*;
use mtg_core::{Step, Zone, ZoneRef};
use mtg_engine::{ChoiceKind, Engine, Progress, choice::Answer, state::GameState};

fn board(hand: usize) -> Engine {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.step = Step::End;
    state.priority = Some(P0);
    for _ in 0..hand {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Hand, P0));
    }
    for p in [P0, P1] {
        for _ in 0..10 {
            state.place(DUMMY, p, ZoneRef::of(Zone::Library, p));
        }
    }
    Engine::new(state)
}

/// Pass until a non-priority question for P0, or until P0's turn is over.
fn to_discard(engine: &mut Engine) -> Option<mtg_engine::Choice> {
    let cards = TestCards::default();
    for _ in 0..2000 {
        if engine.state.active_player == P1 {
            return None;
        }
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => return None,
            Progress::NeedsChoice(c) => {
                if matches!(c.kind, ChoiceKind::ChooseObjects { .. }) {
                    return Some(c);
                }
                engine
                    .answer(&cards, c.id, c.default.clone().unwrap_or(Answer::Pass))
                    .unwrap();
            }
        }
    }
    None
}

#[test]
fn nine_cards_at_cleanup_means_discarding_two() {
    let mut engine = board(9);
    let c = to_discard(&mut engine).expect("asked to discard");
    assert_eq!(c.who, P0);
    let ChoiceKind::ChooseObjects {
        from,
        min: 2,
        max: 2,
    } = &c.kind
    else {
        panic!("{:?}", c.kind)
    };
    let chosen = vec![from[0], from[4]];
    engine
        .answer(&TestCards::default(), c.id, Answer::Objects(chosen))
        .unwrap();
    assert_eq!(
        engine.state.objects_in(ZoneRef::of(Zone::Hand, P0)).len(),
        7
    );
    assert_eq!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .len(),
        2
    );
}

#[test]
fn discarding_the_wrong_number_is_refused() {
    let mut engine = board(9);
    let c = to_discard(&mut engine).unwrap();
    let ChoiceKind::ChooseObjects { from, .. } = &c.kind else {
        panic!()
    };
    assert!(
        engine
            .answer(&TestCards::default(), c.id, Answer::Objects(vec![from[0]]))
            .is_err()
    );
}

#[test]
fn seven_or_fewer_cards_is_no_question() {
    let mut engine = board(7);
    assert!(to_discard(&mut engine).is_none(), "the turn simply ends");
    assert_eq!(
        engine.state.objects_in(ZoneRef::of(Zone::Hand, P0)).len(),
        7
    );
}
