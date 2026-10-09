//! Independent CR 800.4a/d acceptance review (effective 2026-09-25).
//! Source: https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt
mod common;

use common::{creature, gain};
use mtg_core::{AbilityId, CardId, Event, PlayerId, Step, Timestamp, Zone, ZoneRef};
use mtg_engine::{
    Action, Answer, Choice, ChoiceKind, Engine, Progress,
    state::{AffectedSet, ContinuousEffect, GameState},
};
use mtg_ir::{
    Ability, AbilityKind, CardFace, Cost, PrintedCards, Selector,
    ability::ActivationTiming,
    effect::{Duration, Modification},
};

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const P2: PlayerId = PlayerId(2);
const P3: PlayerId = PlayerId(3);

struct Cards(CardFace);
impl PrintedCards for Cards {
    fn face(&self, _: CardId, _: u8) -> Option<&CardFace> {
        Some(&self.0)
    }
    fn subtype_name(&self, _: mtg_core::Subtype) -> Option<&str> {
        None
    }
}
fn cards() -> Cards {
    let mut face = creature("Independent leave-game source", 2, 2, &[]);
    face.abilities.push(Ability {
        id: AbilityId(0),
        targets: vec![],
        source_text: None,
        kind: AbilityKind::Activated {
            cost: Cost::free(),
            effect: gain(2),
            timing: ActivationTiming::Instant,
            is_mana_ability: false,
            is_loyalty_ability: false,
            functions_from: Zone::Battlefield,
        },
    });
    Cards(face)
}
fn state() -> GameState {
    let mut state = GameState::new(&[P0, P1, P2, P3], 20);
    state.turn = 2;
    state.step = Step::PrecombatMain;
    state.active_player = P0;
    state
}
fn choice(engine: &mut Engine, cards: &Cards) -> Choice {
    for _ in 0..1000 {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => return c,
            Progress::GameOver { .. } => panic!("unexpected terminal game"),
        }
    }
    panic!("choice budget exceeded")
}
fn submit(engine: &mut Engine, cards: &Cards, who: PlayerId, action: Action) {
    let c = choice(engine, cards);
    assert_eq!(c.who, who);
    assert!(matches!(c.kind, ChoiceKind::Priority { .. }));
    engine.answer(cards, c.id, Answer::Action(action)).unwrap();
}
fn control(state: &mut GameState, object: mtg_core::ObjectId, by: PlayerId, timestamp: u64) {
    let id = state.new_object_id();
    state.continuous.push(ContinuousEffect {
        id,
        source: object,
        affected: AffectedSet::Fixed(vec![object]),
        modification: Modification::Control(Selector::You),
        duration: Duration::Permanent,
        timestamp: Timestamp(timestamp),
        layer: mtg_engine::layers::layer::CONTROL,
        ability: None,
        controller: Some(by),
    });
    state.generation = Timestamp(timestamp);
    assert_eq!(mtg_engine::layers::controller(state, object), Some(by));
}

#[test]
fn surviving_players_activated_ability_survives_source_owners_departure() {
    let cards = cards();
    let mut state = state();
    let source = state.place(CardId(0), P3, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&source).unwrap().controller = P0;
    let mut engine = Engine::new(state);
    submit(
        &mut engine,
        &cards,
        P0,
        Action::ActivateAbility {
            source,
            ability: AbilityId(0),
        },
    );
    submit(&mut engine, &cards, P0, Action::Pass);
    submit(&mut engine, &cards, P1, Action::Pass);
    submit(&mut engine, &cards, P2, Action::Pass);
    let stack = engine.state.objects_in(ZoneRef::shared(Zone::Stack));
    assert_eq!(stack.len(), 1);
    let ability = stack[0];
    submit(&mut engine, &cards, P3, Action::Concede);
    let _ = choice(&mut engine, &cards);
    assert!(!engine.state.objects.contains_key(&source));
    assert!(
        engine.state.objects.contains_key(&ability),
        "CR113.7a: another player's ability is independent of the removed source"
    );
    for _ in 0..20 {
        if engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
        {
            break;
        }
        let c = choice(&mut engine, &cards);
        engine.answer(&cards, c.id, Answer::Pass).unwrap();
    }
    assert_eq!(engine.state.player(P0).life, 22);
}

#[test]
fn owned_objects_disappear_from_every_zone_without_dying_or_hidden_identity_leaks() {
    let cards = cards();
    let mut state = state();
    let mut owned = Vec::new();
    for zone in [
        Zone::Hand,
        Zone::Library,
        Zone::Graveyard,
        Zone::Battlefield,
        Zone::Exile,
        Zone::Command,
    ] {
        let zone_ref = if zone.is_shared() {
            ZoneRef::shared(zone)
        } else {
            ZoneRef::of(zone, P0)
        };
        owned.push(state.place(CardId(0), P0, zone_ref));
    }
    let mut engine = Engine::new(state);
    submit(&mut engine, &cards, P0, Action::Concede);
    let _ = choice(&mut engine, &cards);
    for id in owned {
        assert!(!engine.state.objects.contains_key(&id));
        assert!(!engine.view_for(P1).visible.contains_key(&id));
        assert!(!engine.log.iter().any(|e| matches!(e.event, Event::ZoneChange { object, to, .. } if object == id && to.zone == Zone::Graveyard)));
    }
}

#[test]
fn simultaneous_departure_ends_both_control_effects_before_exiling_residual_control() {
    let cards = cards();
    let mut state = state();
    let object = state.place(CardId(0), P0, ZoneRef::shared(Zone::Battlefield));
    control(&mut state, object, P2, 1);
    control(&mut state, object, P3, 2);
    state.players.get_mut(&P2).unwrap().life = 0;
    state.players.get_mut(&P3).unwrap().life = 0;
    let mut engine = Engine::new(state);
    let _ = choice(&mut engine, &cards);
    assert!(engine.state.objects.contains_key(&object));
    assert_eq!(
        mtg_engine::layers::controller(&engine.state, object),
        Some(P0)
    );
    assert!(engine.state.continuous.is_empty());
}

#[test]
fn departing_player_control_effect_ends_but_other_players_effect_persists() {
    let cards = cards();
    let mut state = state();
    let object = state.place(CardId(0), P1, ZoneRef::shared(Zone::Battlefield));
    control(&mut state, object, P2, 1);
    control(&mut state, object, P0, 2);
    let mut engine = Engine::new(state);
    submit(&mut engine, &cards, P0, Action::Concede);
    let _ = choice(&mut engine, &cards);
    assert_eq!(
        mtg_engine::layers::controller(&engine.state, object),
        Some(P2)
    );
    assert_eq!(engine.state.objects[&object].owner, P1);
    assert_eq!(engine.state.objects[&object].zone.zone, Zone::Battlefield);
    assert_eq!(engine.state.continuous.len(), 1);
}

#[test]
fn residual_foreign_owned_permanent_is_exiled_after_control_effects_end() {
    let cards = cards();
    let mut state = state();
    let stolen = state.place(CardId(0), P1, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&stolen).unwrap().controller = P0;
    let mut engine = Engine::new(state);
    submit(&mut engine, &cards, P0, Action::Concede);
    let _ = choice(&mut engine, &cards);
    assert!(!engine.state.objects.contains_key(&stolen));
    let exile = engine.state.objects_in(ZoneRef::shared(Zone::Exile));
    assert_eq!(exile.len(), 1);
    assert_eq!(engine.state.objects[&exile[0]].owner, P1);
    assert!(engine.log.iter().any(|event| matches!(event.event,
        Event::ZoneChange { object, to, .. } if object == stolen && to.zone == Zone::Exile)));
}

#[test]
fn pending_delayed_triggers_follow_controller_not_source_owner() {
    use mtg_core::{Cause, EventId, StampedEvent};
    use mtg_engine::PendingTrigger;
    let cards = cards();
    let mut state = state();
    let source = state.place(CardId(0), P3, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&source).unwrap().controller = P0;
    state.players.get_mut(&P3).unwrap().life = 0;
    for (number, controller) in [(1, P0), (2, P3)] {
        state.pending_triggers.push(PendingTrigger {
            source,
            card: CardId(0),
            face: 0,
            ability: AbilityId(0),
            controller,
            cause: StampedEvent {
                id: EventId(number),
                at: Timestamp(0),
                cause: Cause::TurnStructure,
                event: Event::StepBegan {
                    turn: 2,
                    active: P0,
                    step: Step::PrecombatMain,
                },
            },
            fired_at: Timestamp(0),
            footprint: Default::default(),
            bindings: Default::default(),
            delayed: Some((number as u32, gain(2))),
            granted: None,
        });
    }
    let mut engine = Engine::new(state);
    let _ = choice(&mut engine, &cards);
    let stack = engine.state.objects_in(ZoneRef::shared(Zone::Stack));
    assert_eq!(stack.len(), 1);
    assert_eq!(engine.state.objects[&stack[0]].controller, P0);
    assert!(!engine.log.iter().any(|event| matches!(event.event,
        Event::AbilityPutOnStack { controller, .. } if controller == P3)));
    for _ in 0..20 {
        if engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
        {
            break;
        }
        let c = choice(&mut engine, &cards);
        engine.answer(&cards, c.id, Answer::Pass).unwrap();
    }
    assert_eq!(engine.state.player(P0).life, 22);
    assert_eq!(engine.state.player(P3).life, 0);
}

#[test]
fn resolved_noncontrol_effect_on_survivor_outlasts_departed_source() {
    let cards = cards();
    let mut state = state();
    let source = state.place(CardId(0), P0, ZoneRef::shared(Zone::Battlefield));
    let survivor = state.place(CardId(0), P1, ZoneRef::shared(Zone::Battlefield));
    let effect = state.new_object_id();
    state.continuous.push(ContinuousEffect {
        id: effect,
        source,
        affected: AffectedSet::Fixed(vec![survivor]),
        modification: Modification::ModifyPowerToughness {
            power: mtg_ir::Value::Fixed(3),
            toughness: mtg_ir::Value::Fixed(3),
        },
        duration: Duration::UntilEndOfTurn,
        timestamp: Timestamp(1),
        layer: mtg_engine::layers::layer::PT_MODIFY,
        ability: None,
        controller: Some(P0),
    });
    assert_eq!(
        mtg_engine::layers::compute(&state, &cards, survivor)
            .unwrap()
            .power,
        Some(5)
    );
    let mut engine = Engine::new(state);
    submit(&mut engine, &cards, P0, Action::Concede);
    let _ = choice(&mut engine, &cards);
    assert!(!engine.state.objects.contains_key(&source));
    assert_eq!(
        mtg_engine::layers::compute(&engine.state, &cards, survivor)
            .unwrap()
            .power,
        Some(5)
    );
    assert_eq!(engine.state.objects[&survivor].owner, P1);
}
