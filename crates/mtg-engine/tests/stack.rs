//! The stack (CR 405, CR 608).
//!
//! The properties that make it a stack rather than a queue, pinned down so a
//! refactor cannot quietly invert them: last in resolves first, exactly one object
//! resolves per round of passes, and players get priority in between.

mod common;

use common::*;
use mtg_core::{Event, ObjectId, Step, Zone, ZoneRef};
use mtg_engine::{
    Engine, Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
    state::GameState,
};

fn board() -> GameState {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.step = Step::PrecombatMain;
    state.priority = Some(P0);
    for _ in 0..6 {
        state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    }
    state
}

fn stack(engine: &Engine) -> Vec<ObjectId> {
    engine.state.objects_in(ZoneRef::shared(Zone::Stack))
}

/// Cast the named objects in order, holding priority (never passing) in between.
fn cast_all(engine: &mut Engine, cards: &TestCards, want: &[ObjectId]) -> Vec<ObjectId> {
    let mut cast_as = Vec::new();
    let mut remaining: Vec<ObjectId> = want.to_vec();

    for _ in 0..4000 {
        if remaining.is_empty() {
            return cast_as;
        }
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
                let pick = match &c.kind {
                    ChoiceKind::Priority { legal } => legal
                        .actions
                        .iter()
                        .find(
                            |a| matches!(a, Action::Cast { object } if remaining.contains(object)),
                        )
                        .cloned(),
                    _ => None,
                };
                match pick {
                    Some(Action::Cast { object }) => {
                        remaining.retain(|o| *o != object);
                        engine
                            .answer(cards, c.id, Answer::Action(Action::Cast { object }))
                            .unwrap();
                        // The spell takes a new id on the stack: the newest stack
                        // object is it.
                        if let Some(top) = stack(engine).first() {
                            cast_as.push(*top);
                        }
                    }
                    _ => {
                        engine.answer(cards, c.id, Answer::Pass).unwrap();
                    }
                }
            }
        }
    }
    cast_as
}

/// Order in which `Resolved` events appear in the log.
fn resolution_order(engine: &Engine) -> Vec<ObjectId> {
    engine
        .log
        .iter()
        .filter_map(|e| match e.event {
            Event::Resolved { object } => Some(object),
            _ => None,
        })
        .collect()
}

#[test]
fn a_cast_spell_goes_on_the_stack() {
    let mut state = board();
    let spell = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let on_stack = cast_all(&mut engine, &cards, &[spell]);
    assert_eq!(on_stack.len(), 1, "the spell should have reached the stack");
    assert_eq!(stack(&engine).len(), 1);
}

#[test]
fn two_spells_resolve_last_in_first_out() {
    // The defining property. Cast small then big; big must resolve first.
    let mut state = board();
    let small = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let big = state.place(BIG_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let ids = cast_all(&mut engine, &cards, &[small, big]);
    assert_eq!(ids.len(), 2, "both spells should be on the stack");
    let (small_on_stack, big_on_stack) = (ids[0], ids[1]);
    assert_eq!(
        stack(&engine),
        vec![big_on_stack, small_on_stack],
        "newest is on top"
    );

    run(&mut engine, &cards, 4000, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).is_empty()
    });

    let order = resolution_order(&engine);
    let big_at = order
        .iter()
        .position(|o| *o == big_on_stack)
        .expect("big resolved");
    let small_at = order
        .iter()
        .position(|o| *o == small_on_stack)
        .expect("small resolved");
    assert!(
        big_at < small_at,
        "the spell cast second must resolve first: {order:?}"
    );
}

#[test]
fn both_spells_effects_actually_happen() {
    let mut state = board();
    let small = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let big = state.place(BIG_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    cast_all(&mut engine, &cards, &[small, big]);
    run(&mut engine, &cards, 4000, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).is_empty()
    });

    assert_eq!(engine.state.player(P0).life, 31, "20 + 10 + 1");
}

#[test]
fn only_one_object_resolves_per_round_of_passes() {
    // CR 117.4: all players passing resolves the top object, and then priority comes
    // back — it does not drain the stack.
    let mut state = board();
    let small = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let big = state.place(BIG_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    cast_all(&mut engine, &cards, &[small, big]);
    assert_eq!(stack(&engine).len(), 2);

    // Everyone passes once.
    let mut passes = 0;
    for _ in 0..200 {
        if stack(&engine).len() < 2 {
            break;
        }
        match engine.advance(&cards) {
            Progress::NeedsChoice(c) => {
                engine.answer(&cards, c.id, Answer::Pass).unwrap();
                passes += 1;
            }
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
        }
    }
    assert!(passes >= 2, "both players should have had to pass");
    assert_eq!(
        stack(&engine).len(),
        1,
        "exactly one object resolved, not both"
    );
}

#[test]
fn a_resolved_instant_goes_to_its_owners_graveyard() {
    let mut state = board();
    let spell = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    cast_all(&mut engine, &cards, &[spell]);
    run(&mut engine, &cards, 4000, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).is_empty()
    });

    assert_eq!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .len(),
        1,
        "an instant goes to the graveyard after resolving"
    );
}

#[test]
fn a_resolved_creature_spell_goes_to_the_battlefield() {
    let mut state = board();
    let spell = state.place(COSTED, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    cast_all(&mut engine, &cards, &[spell]);
    run(&mut engine, &cards, 4000, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).is_empty()
    });

    let creatures = engine
        .state
        .battlefield()
        .iter()
        .filter(|id| {
            mtg_engine::layers::compute(&engine.state, &cards, **id)
                .is_some_and(|c| c.has_type(mtg_core::CardType::Creature))
        })
        .count();
    assert_eq!(creatures, 1);
    assert_eq!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .len(),
        0
    );
}

#[test]
fn a_triggered_ability_uses_the_stack_too() {
    // Triggers are stack objects, not immediate effects.
    use mtg_core::{AbilityId, Cause, EventId, StampedEvent, Timestamp};
    use mtg_engine::PendingTrigger;
    use mtg_ir::footprint::{Footprint, Resource};

    let mut state = board();
    let source = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    state.pending_triggers.push(PendingTrigger {
        source,
        card: DUMMY,
        face: 0,
        ability: AbilityId(0),
        controller: P0,
        cause: StampedEvent {
            id: EventId(1),
            at: Timestamp(1),
            cause: Cause::TurnStructure,
            event: Event::Shuffled { player: P0 },
        },
        fired_at: Timestamp(1),
        footprint: Footprint {
            writes: [Resource::Object(source)].into_iter().collect(),
            ..Default::default()
        },
        bindings: Default::default(),
        delayed: None,
        granted: None,
    });

    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    run(&mut engine, &cards, 200, |e| {
        !e.state.objects_in(ZoneRef::shared(Zone::Stack)).is_empty()
    });

    assert_eq!(
        stack(&engine).len(),
        1,
        "the trigger should be on the stack"
    );
}
