//! CR 103.5, 103.5c and 103.8: declaration rounds, London bottoming and multiplayer.
#[path = "../../mtg-engine/tests/common/mod.rs"]
mod common;
use common::*;
use mtg_core::{ObjectId, PlayerId, Step, Zone, ZoneRef};
use mtg_engine::{
    Choice, ChoiceKind, Engine, Progress,
    choice::Answer,
    state::{GameState, Pregame, Rng},
};
use mtg_verify::canonical::CanonicalGameState;

fn game(players: u8, first: u8) -> Engine {
    let seats: Vec<_> = (0..players).map(PlayerId).collect();
    let mut state = GameState::new(&seats, 40);
    state.turn = 1;
    state.step = Step::Untap;
    state.active_player = PlayerId(first);
    state.priority = Some(PlayerId(first));
    state.turn_order.rotate_left(first as usize);
    state.pregame = Some(Pregame::new(state.turn_order.clone()));
    state.rng = Rng::from_seed(&[31; 32]);
    for seat in seats {
        for i in 0..40 {
            state.place(
                if i % 2 == 0 { DUMMY } else { FIELD },
                seat,
                ZoneRef::of(if i < 7 { Zone::Hand } else { Zone::Library }, seat),
            );
        }
    }
    Engine::new(state)
}
fn next(engine: &mut Engine) -> Choice {
    for _ in 0..1000 {
        match engine.advance(&TestCards::default()) {
            Progress::NeedsChoice(choice) => return choice,
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("unexpected game over"),
        }
    }
    panic!("opening-hand progress budget exhausted")
}
fn hand(engine: &Engine, who: PlayerId) -> Vec<ObjectId> {
    engine.state.objects_in(ZoneRef::of(Zone::Hand, who))
}
fn declare(engine: &mut Engine, who: PlayerId, taken: u32, keep: bool) {
    let choice = next(engine);
    assert_eq!(choice.who, who);
    assert!(
        matches!(choice.kind, ChoiceKind::KeepOrMulligan { mulligans_taken } if mulligans_taken == taken)
    );
    engine
        .answer(&TestCards::default(), choice.id, Answer::Bool(keep))
        .unwrap();
}
fn bottom(engine: &mut Engine, who: PlayerId, count: u32) {
    let choice = next(engine);
    assert_eq!(choice.who, who);
    let ChoiceKind::ChooseObjects { from, min, max } = choice.kind else {
        panic!("bottom choice required")
    };
    assert_eq!((min, max), (count, count));
    // Input order is the selected bottom order, rather than the original hand order.
    let selected: Vec<_> = from.iter().rev().take(count as usize).copied().collect();
    let before = CanonicalGameState::from_engine(engine).unwrap();
    assert!(
        engine
            .answer(&TestCards::default(), choice.id, Answer::Objects(vec![]))
            .is_err()
    );
    assert_eq!(CanonicalGameState::from_engine(engine).unwrap(), before);
    assert_eq!(next(engine).id, choice.id);
    engine
        .answer(
            &TestCards::default(),
            choice.id,
            Answer::Objects(selected.clone()),
        )
        .unwrap();
    assert_eq!(hand(engine, who).len(), 7 - count as usize);
    let library = engine.state.objects_in(ZoneRef::of(Zone::Library, who));
    assert_eq!(&library[library.len() - count as usize..], &selected);
}

#[test]
fn declarations_finish_before_redraw_and_kept_players_do_not_reenter_rounds() {
    for players in 2..=4 {
        let first = players - 1;
        let mut engine = game(players, first);
        let original = hand(&engine, PlayerId(first));
        declare(&mut engine, PlayerId(first), 0, false);
        for seat in 0..players - 1 {
            assert_eq!(
                hand(&engine, PlayerId(first)),
                original,
                "no early replacement hand"
            );
            declare(&mut engine, PlayerId(seat), 0, true);
        }
        if players == 2 {
            bottom(&mut engine, PlayerId(first), 1);
        }
        declare(&mut engine, PlayerId(first), 1, true);
        assert_eq!(
            hand(&engine, PlayerId(first)).len(),
            if players == 2 { 6 } else { 7 }
        );
        for seat in 0..players - 1 {
            assert_eq!(hand(&engine, PlayerId(seat)).len(), 7);
        }
        assert!(matches!(
            next(&mut engine).kind,
            ChoiceKind::Priority { .. }
        ));
        assert!(engine.state.pregame.is_none());
    }
}

#[test]
fn each_multiplayer_seat_gets_one_free_mulligan_then_bottoms_before_deciding() {
    for players in 3..=4 {
        let mut engine = game(players, 0);
        for seat in 0..players {
            declare(&mut engine, PlayerId(seat), 0, false);
        }
        for seat in 0..players {
            declare(&mut engine, PlayerId(seat), 1, false);
            assert_eq!(
                hand(&engine, PlayerId(seat)).len(),
                7,
                "first redraw is free"
            );
        }
        for seat in 0..players {
            bottom(&mut engine, PlayerId(seat), 1);
        }
        for seat in 0..players {
            declare(&mut engine, PlayerId(seat), 2, true);
        }
        assert!(matches!(
            next(&mut engine).kind,
            ChoiceKind::Priority { .. }
        ));
        for seat in 0..players {
            assert_eq!(hand(&engine, PlayerId(seat)).len(), 6);
        }
    }
}

#[test]
fn zero_card_limit_counts_only_paid_mulligans() {
    for players in 2..=4 {
        let mut engine = game(players, 0);
        declare(&mut engine, P0, 0, false);
        for seat in 1..players {
            declare(&mut engine, PlayerId(seat), 0, true);
        }
        let free = u32::from(players > 2);
        for taken in 1..=7 + free {
            let count = taken - free;
            if count > 0 {
                bottom(&mut engine, P0, count);
            }
            // A false answer at the zero-card limit must be treated as a keep.
            declare(&mut engine, P0, taken, false);
        }
        assert!(matches!(
            next(&mut engine).kind,
            ChoiceKind::Priority { .. }
        ));
        assert_eq!(hand(&engine, P0).len(), 0);
        assert_eq!(
            engine
                .state
                .objects_in(ZoneRef::of(Zone::Library, P0))
                .len(),
            40
        );
    }
}

#[test]
fn multiplayer_starting_player_draws_but_two_player_starting_player_skips() {
    for players in 2..=4 {
        let mut engine = game(players, 0);
        for seat in 0..players {
            declare(&mut engine, PlayerId(seat), 0, true);
        }
        for _ in 0..50 {
            let choice = next(&mut engine);
            if engine.state.step == Step::PrecombatMain {
                break;
            }
            engine
                .answer(&TestCards::default(), choice.id, Answer::Pass)
                .unwrap();
        }
        assert_eq!(engine.state.step, Step::PrecombatMain);
        assert_eq!(hand(&engine, P0).len(), if players == 2 { 7 } else { 8 });
    }
}
