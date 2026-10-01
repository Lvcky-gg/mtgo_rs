//! Taking back priority actions: what can be undone, and where undo must stop.

mod common;

use common::*;
use mtg_core::{Step, Zone, ZoneRef};
use mtg_engine::{
    Choice, Engine, Illegal, Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
    state::GameState,
};

fn board() -> GameState {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.step = Step::Untap;
    state.priority = Some(P0);
    for _ in 0..10 {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Library, P0));
        state.place(DUMMY, P1, ZoneRef::of(Zone::Library, P1));
    }
    state
}

/// The next question, whoever it is for.
fn next(engine: &mut Engine, cards: &TestCards) -> Choice {
    loop {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => return c,
            Progress::GameOver { .. } => panic!("game ended"),
        }
    }
}

/// Drive to P0's first priority in the precombat main phase.
fn main_phase(engine: &mut Engine, cards: &TestCards) -> Choice {
    for _ in 0..5000 {
        let c = next(engine, cards);
        if matches!(c.kind, ChoiceKind::Priority { .. })
            && c.who == P0
            && engine.state.step == Step::PrecombatMain
        {
            return c;
        }
        engine
            .answer(cards, c.id, c.default.clone().unwrap_or(Answer::Pass))
            .unwrap();
    }
    panic!("never reached P0's main phase");
}

fn find(choice: &Choice, want: impl Fn(&Action) -> bool) -> Action {
    let ChoiceKind::Priority { legal } = &choice.kind else {
        panic!("not priority")
    };
    legal
        .all()
        .find(|a| want(a))
        .cloned()
        .expect("action offered")
}

fn tapped(engine: &Engine) -> usize {
    engine
        .state
        .battlefield()
        .iter()
        .filter(|id| engine.state.objects[id].tapped)
        .count()
}

#[test]
fn nothing_to_undo_at_the_start_of_a_turn() {
    let mut engine = Engine::new(board());
    let cards = TestCards::default();
    let choice = main_phase(&mut engine, &cards);
    assert!(!choice.undo, "the question says undo is unavailable");
    assert_eq!(
        engine.answer(&cards, choice.id, Answer::Undo),
        Err(Illegal::CannotUndo)
    );
    // A refused undo leaves the question standing.
    assert_eq!(next(&mut engine, &cards).id, choice.id);
}

#[test]
fn a_mana_tap_can_be_undone() {
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let choice = main_phase(&mut engine, &cards);
    let tap = find(&choice, |a| matches!(a, Action::ActivateManaAbility { .. }));
    engine
        .answer(&cards, choice.id, Answer::Action(tap))
        .unwrap();
    assert_eq!(tapped(&engine), 1);

    let after = next(&mut engine, &cards);
    assert!(after.undo, "the question after tapping offers undo");
    engine.answer(&cards, after.id, Answer::Undo).unwrap();

    assert_eq!(tapped(&engine), 0, "untapped again");
    assert_eq!(
        engine.state.player(P0).mana.total(),
        0,
        "and the mana is gone"
    );
    let again = next(&mut engine, &cards);
    assert!(again.id > after.id, "asked again under a fresh id");
    assert!(matches!(again.kind, ChoiceKind::Priority { .. }));
}

#[test]
fn several_actions_undo_one_at_a_time() {
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let mut choice = main_phase(&mut engine, &cards);
    for _ in 0..2 {
        let tap = find(&choice, |a| matches!(a, Action::ActivateManaAbility { .. }));
        engine
            .answer(&cards, choice.id, Answer::Action(tap))
            .unwrap();
        choice = next(&mut engine, &cards);
    }
    assert_eq!(tapped(&engine), 2);

    engine.answer(&cards, choice.id, Answer::Undo).unwrap();
    assert_eq!(tapped(&engine), 1);
    let choice = next(&mut engine, &cards);
    assert!(choice.undo, "one more to go");
    engine.answer(&cards, choice.id, Answer::Undo).unwrap();
    assert_eq!(tapped(&engine), 0);
    assert!(
        !next(&mut engine, &cards).undo,
        "back to where priority began"
    );
}

#[test]
fn playing_a_land_can_be_undone_and_the_land_drop_returns() {
    let mut state = board();
    let land = state.place(FIELD, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let choice = main_phase(&mut engine, &cards);
    let play = find(
        &choice,
        |a| matches!(a, Action::PlayLand { object } if *object == land),
    );
    engine
        .answer(&cards, choice.id, Answer::Action(play.clone()))
        .unwrap();
    assert_eq!(engine.state.player(P0).lands_played, 1);

    let after = next(&mut engine, &cards);
    engine.answer(&cards, after.id, Answer::Undo).unwrap();
    assert_eq!(engine.state.player(P0).lands_played, 0);
    assert_eq!(
        engine.state.objects[&land].zone,
        ZoneRef::of(Zone::Hand, P0),
        "back in hand"
    );
    let again = next(&mut engine, &cards);
    let ChoiceKind::Priority { legal } = &again.kind else {
        panic!()
    };
    assert!(legal.actions.contains(&play), "and playable again");
}

#[test]
fn a_cast_is_undone_whole_lands_and_all() {
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let spell = state.place(COSTED, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let choice = main_phase(&mut engine, &cards);
    let cast = find(
        &choice,
        |a| matches!(a, Action::Cast { object } if *object == spell),
    );
    engine
        .answer(&cards, choice.id, Answer::Action(cast))
        .unwrap();
    assert_eq!(tapped(&engine), 2, "auto-tapped to pay");

    let after = next(&mut engine, &cards);
    assert_eq!(
        after.who, P0,
        "the caster holds priority with the spell on the stack"
    );
    engine.answer(&cards, after.id, Answer::Undo).unwrap();

    assert_eq!(tapped(&engine), 0);
    assert!(
        engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
    );
    assert!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .contains(&spell)
    );
}

#[test]
fn once_the_opponent_is_asked_the_action_stands() {
    let mut state = board();
    let land = state.place(FIELD, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let choice = main_phase(&mut engine, &cards);
    let play = find(
        &choice,
        |a| matches!(a, Action::PlayLand { object } if *object == land),
    );
    engine
        .answer(&cards, choice.id, Answer::Action(play))
        .unwrap();

    // Pass; the opponent is asked, and may have reacted to the land.
    let mine = next(&mut engine, &cards);
    engine.answer(&cards, mine.id, Answer::Pass).unwrap();
    let theirs = next(&mut engine, &cards);
    assert_eq!(theirs.who, P1);
    assert!(!theirs.undo, "the opponent has nothing of theirs to undo");
    engine.answer(&cards, theirs.id, Answer::Pass).unwrap();

    let back = next(&mut engine, &cards);
    assert!(!back.undo, "the land drop was seen, so it stays");
    assert!(engine.answer(&cards, back.id, Answer::Undo).is_err());
}

#[test]
fn a_player_cannot_undo_someone_elses_action() {
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let choice = main_phase(&mut engine, &cards);
    let tap = find(&choice, |a| matches!(a, Action::ActivateManaAbility { .. }));
    engine
        .answer(&cards, choice.id, Answer::Action(tap))
        .unwrap();
    let mine = next(&mut engine, &cards);
    engine.answer(&cards, mine.id, Answer::Pass).unwrap();

    let theirs = next(&mut engine, &cards);
    assert_eq!(theirs.who, P1);
    assert_eq!(
        engine.answer(&cards, theirs.id, Answer::Undo),
        Err(Illegal::CannotUndo)
    );
    assert_eq!(tapped(&engine), 1);
}

#[test]
fn undo_stops_at_anything_that_revealed_information() {
    // A draw is the plainest case: undoing past it would let a player see a card and
    // then choose differently. Simulated here by a draw event after the checkpoint.
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let choice = main_phase(&mut engine, &cards);
    let tap = find(&choice, |a| matches!(a, Action::ActivateManaAbility { .. }));
    engine
        .answer(&cards, choice.id, Answer::Action(tap))
        .unwrap();
    assert!(engine.can_undo(P0));

    mtg_engine::apply::apply(
        &mut engine.state,
        mtg_core::Cause::TurnStructure,
        mtg_core::Event::Drew {
            player: P0,
            object: mtg_core::ObjectId(1),
        },
        &mut engine.log,
    );
    assert!(
        !engine.can_undo(P0),
        "a draw since the tap closes the window"
    );
}
