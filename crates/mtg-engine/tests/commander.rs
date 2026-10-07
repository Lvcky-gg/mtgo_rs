//! Commander (CR 903): the command zone, commander tax, and commander damage.

mod common;

use common::*;
use mtg_core::{Cause, Event, LossReason, ObjectId, Step, Zone, ZoneRef};
use mtg_engine::{
    Choice, Engine, Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
    state::GameState,
};

/// A Commander game: P0's commander is the (free-costed) legendary test creature.
fn board() -> GameState {
    let mut state = GameState::new(&[P0, P1], 40);
    state.turn = 2;
    state.active_player = P0;
    state.step = Step::Untap;
    state.priority = Some(P0);
    for _ in 0..10 {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Library, P0));
        state.place(DUMMY, P1, ZoneRef::of(Zone::Library, P1));
    }
    state.commander.commanders.insert(P0, LEGEND);
    state
}

fn main_phase(engine: &mut Engine, cards: &TestCards) -> Choice {
    for _ in 0..5000 {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
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
        }
    }
    panic!("never reached P0's main phase");
}

/// The commander, if casting it is offered. (The library is free-costed cards, and one is
/// drawn each turn, so "any cast offered" is not the question.)
fn offered_cast(engine: &Engine, choice: &Choice) -> Option<ObjectId> {
    let ChoiceKind::Priority { legal } = &choice.kind else {
        return None;
    };
    legal.actions.iter().find_map(|a| match a {
        Action::Cast { object } if engine.state.objects[object].card == LEGEND => Some(*object),
        _ => None,
    })
}

fn where_is_commander(engine: &Engine) -> Vec<Zone> {
    engine
        .state
        .objects
        .values()
        .filter(|o| o.card == LEGEND && o.owner == P0)
        .map(|o| o.zone.zone)
        .collect()
}

#[test]
fn a_commander_can_be_cast_from_the_command_zone() {
    let mut state = board();
    let commander = state.place(LEGEND, P0, ZoneRef::shared(Zone::Command));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let choice = main_phase(&mut engine, &cards);
    assert_eq!(offered_cast(&engine, &choice), Some(commander));
    engine
        .answer(
            &cards,
            choice.id,
            Answer::Action(Action::Cast { object: commander }),
        )
        .unwrap();
    assert_eq!(
        engine.state.commander.casts[&P0], 1,
        "the cast is counted for tax"
    );
}

#[test]
fn the_second_cast_costs_two_more() {
    let mut state = board();
    state.place(LEGEND, P0, ZoneRef::shared(Zone::Command));
    state.commander.casts.insert(P0, 1);
    let mut engine = Engine::new(state.clone());
    let cards = TestCards::default();
    let choice = main_phase(&mut engine, &cards);
    assert_eq!(
        offered_cast(&engine, &choice),
        None,
        "no mana for the {{2}} tax"
    );

    // Two lands pay the tax on a free commander.
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let choice = main_phase(&mut engine, &cards);
    let commander = offered_cast(&engine, &choice).expect("affordable with the tax paid");
    engine
        .answer(
            &cards,
            choice.id,
            Answer::Action(Action::Cast { object: commander }),
        )
        .unwrap();
    let tapped = engine
        .state
        .battlefield()
        .iter()
        .filter(|id| engine.state.objects[id].tapped)
        .count();
    assert_eq!(tapped, 2, "both lands paid the tax");
}

#[test]
fn a_dead_commander_goes_back_to_the_command_zone() {
    let mut state = board();
    state.place(LEGEND, P0, ZoneRef::of(Zone::Graveyard, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    main_phase(&mut engine, &cards);
    assert_eq!(where_is_commander(&engine), vec![Zone::Command]);
}

#[test]
fn an_ordinary_card_in_a_graveyard_stays_there() {
    let mut state = board();
    state.place(DUMMY, P0, ZoneRef::of(Zone::Graveyard, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    main_phase(&mut engine, &cards);
    assert!(
        !engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .is_empty()
    );
}

#[test]
fn combat_damage_from_a_commander_is_tallied_and_other_damage_is_not() {
    let mut state = board();
    let commander = state.place(LEGEND, P0, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let hit = |()| Event::DamageDealtToPlayer {
        source: commander,
        player: P1,
        amount: 5,
        counters: false,
    };

    mtg_engine::apply::apply(&mut engine.state, Cause::Combat, hit(()), &mut engine.log);
    mtg_engine::apply::apply(
        &mut engine.state,
        Cause::Resolution(commander),
        hit(()),
        &mut engine.log,
    );
    assert_eq!(
        engine.state.commander.damage[&(P1, P0)],
        5,
        "combat only (CR 903.10a)"
    );
}

#[test]
fn twenty_one_commander_damage_loses_the_game() {
    let mut state = board();
    state.commander.damage.insert((P1, P0), 21);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    for _ in 0..100 {
        if let Progress::GameOver { winners } = engine.advance(&cards) {
            assert_eq!(winners, vec![P0]);
            assert!(engine.log.iter().any(|e| matches!(
                e.event,
                Event::Lost {
                    player: P1,
                    reason: LossReason::CommanderDamage
                }
            )));
            assert!(engine.state.player(P1).life > 0, "with life to spare");
            return;
        }
    }
    panic!("the game should have ended");
}

#[test]
fn each_player_sees_the_commander_damage_they_have_taken() {
    let mut state = board();
    state.commander.damage.insert((P1, P0), 9);
    let view = mtg_engine::view::project(&state, P1);
    assert_eq!(view.players[&P1].commander_damage.get(&P0), Some(&9));
    assert!(
        view.players[&P0].commander_damage.is_empty(),
        "P0 has taken none"
    );
}
