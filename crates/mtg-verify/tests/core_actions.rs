mod support;

use mtg_core::{CardId, Cause, Event, ObjectId, PlayerId, Target, Zone, ZoneRef};
use mtg_engine::{
    layers::PrintedCards,
    resolve::{self, ResolveCtx, ResolveError},
    state::GameState,
};
use mtg_ir::{CardFace, Effect, Selector, Value, effect::ZonePosition};
use support::*;

struct Cards(Vec<CardFace>);
impl PrintedCards for Cards {
    fn face(&self, card: CardId, _face: u8) -> Option<&CardFace> {
        self.0.get(card.0 as usize)
    }
    fn subtype_name(&self, _subtype: mtg_core::Subtype) -> Option<&str> {
        None
    }
}

struct World {
    state: GameState,
    cards: Cards,
    log: Vec<mtg_core::StampedEvent>,
    context: ResolveCtx,
}
impl World {
    fn new() -> Self {
        let mut state = main_state();
        let source = state.place(CardId(0), P0, ZoneRef::shared(Zone::Battlefield));
        let faces = (0..4)
            .map(|i| {
                let mut face = support::Cards::creature(5, false).0;
                face.name = format!("Independent primitive fixture {i}").into();
                face
            })
            .collect();
        Self {
            state,
            cards: Cards(faces),
            log: vec![],
            context: ResolveCtx::new(source, P0),
        }
    }
    fn place(&mut self, card: u32, player: PlayerId, zone: Zone) -> ObjectId {
        self.state.place(
            CardId(card),
            player,
            if zone.is_shared() {
                ZoneRef::shared(zone)
            } else {
                ZoneRef::of(zone, player)
            },
        )
    }
    fn target(&mut self, target: Target) {
        self.context.targets = vec![target];
        self.context.target_legal = vec![true];
    }
    fn run(&mut self, effect: Effect) {
        resolve::resolve(
            &mut self.state,
            &self.cards,
            &mut self.log,
            &effect,
            &mut self.context,
        )
        .unwrap();
    }
    fn moved(&self, original: ObjectId) -> ObjectId {
        self.log
            .iter()
            .find_map(|entry| match entry.event {
                Event::ZoneChange {
                    object, new_object, ..
                } if object == original => Some(new_object),
                _ => None,
            })
            .expect("expected zone-change event")
    }
}
fn selected() -> Selector {
    Selector::Target { index: 0 }
}
fn movement(what: Selector, to: Zone) -> Effect {
    Effect::MoveZone {
        what,
        to,
        owner_relative_to: None,
        position: ZonePosition::Natural,
        tapped: false,
        face_down: false,
        under_control_of: None,
    }
}

#[test]
fn cr_121_draws_are_individual_top_library_to_hand_moves_with_new_identity() {
    let mut world = World::new();
    let first = world.place(1, P1, Zone::Library);
    let second = world.place(2, P1, Zone::Library);
    let third = world.place(3, P1, Zone::Library);
    world.target(Target::Player(P1));
    world.run(Effect::Draw {
        who: selected(),
        count: Value::Fixed(2),
    });
    assert_eq!(
        world.state.objects_in(ZoneRef::of(Zone::Library, P1)),
        vec![third]
    );
    for (original, card) in [(first, CardId(1)), (second, CardId(2))] {
        assert!(!world.state.objects.contains_key(&original));
        let new = world.moved(original);
        assert_ne!(new, original);
        assert_eq!(world.state.objects[&new].card, card);
        assert_eq!(world.state.objects[&new].owner, P1);
        assert_eq!(world.state.objects[&new].zone, ZoneRef::of(Zone::Hand, P1));
    }
    let draws: Vec<_> = world
        .log
        .iter()
        .filter_map(|entry| match entry.event {
            Event::Drew { player, object } => {
                assert_eq!(player, P1);
                Some(object)
            }
            _ => None,
        })
        .collect();
    assert_eq!(draws, vec![world.moved(first), world.moved(second)]);
    assert_eq!(world.state.objects.len(), 4, "physical-card conservation");
    assert!(
        world
            .log
            .iter()
            .all(|entry| entry.cause == Cause::Resolution(world.context.source))
    );
}

#[test]
fn cr_701_8_discard_is_chosen_by_affected_player_into_owners_graveyard() {
    let mut world = World::new();
    let own = world.place(1, P0, Zone::Hand);
    let kept = world.place(1, P1, Zone::Hand);
    let chosen = world.place(2, P1, Zone::Hand);
    world.target(Target::Player(P1));
    let effect = Effect::Discard {
        who: selected(),
        count: Value::Fixed(1),
        at_random: false,
    };
    let request = resolve::resolve(
        &mut world.state,
        &world.cards,
        &mut world.log,
        &effect,
        &mut world.context,
    );
    assert!(matches!(request, Err(ResolveError::Ask { who, .. }) if who == P1));
    assert!(
        world.log.is_empty(),
        "asking must not discard before player selection"
    );
    world.context.answers = vec![mtg_engine::choice::Answer::Objects(vec![chosen])];
    world.run(effect);
    assert!(world.state.objects.contains_key(&own));
    assert!(world.state.objects.contains_key(&kept));
    assert!(!world.state.objects.contains_key(&chosen));
    let discarded = world.moved(chosen);
    assert_eq!(
        world.state.objects[&discarded].zone,
        ZoneRef::of(Zone::Graveyard, P1)
    );
    assert_eq!(world.state.objects[&discarded].owner, P1);
    assert_eq!(world.state.objects[&discarded].card, CardId(2));
    assert_eq!(world.state.objects.len(), 4);
    assert!(
        !world
            .log
            .iter()
            .any(|entry| matches!(entry.event, Event::Drew { .. }))
    );
}

#[test]
fn cr_701_13_mill_uses_top_cards_and_empty_library_does_not_imply_draw_loss() {
    let mut world = World::new();
    let originals: Vec<_> = (1..=3)
        .map(|card| world.place(card, P1, Zone::Library))
        .collect();
    world.target(Target::Player(P1));
    let mill = |count| {
        movement(
            Selector::TopOfLibrary {
                player: Box::new(selected()),
                count: Value::Fixed(count),
            },
            Zone::Graveyard,
        )
    };
    world.run(mill(2));
    assert_eq!(
        world.state.objects_in(ZoneRef::of(Zone::Library, P1)),
        vec![originals[2]]
    );
    assert_eq!(
        world
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P1))
            .len(),
        2
    );
    world.run(mill(10));
    assert!(
        world
            .state
            .objects_in(ZoneRef::of(Zone::Library, P1))
            .is_empty()
    );
    assert_eq!(
        world
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P1))
            .len(),
        3
    );
    assert_eq!(world.state.objects.len(), 4);
    for original in originals {
        assert!(!world.state.objects.contains_key(&original));
    }
    assert!(!world.log.iter().any(|entry| matches!(
        entry.event,
        Event::Drew { .. } | Event::AttemptedDrawFromEmptyLibrary { .. }
    )));
    let mut engine = mtg_engine::Engine::new(world.state);
    let priority = mtg_verify::scenario::next_choice(&mut engine, &world.cards)
        .unwrap()
        .unwrap();
    assert!(matches!(
        priority.kind,
        mtg_engine::choice::ChoiceKind::Priority { .. }
    ));
    assert!(!engine.state.players[&P1].has_lost);
}

#[test]
fn cr_119_life_gain_loss_are_not_damage_events() {
    let mut world = World::new();
    world.target(Target::Player(P1));
    world.run(Effect::GainLife {
        who: selected(),
        amount: Value::Fixed(7),
    });
    world.run(Effect::LoseLife {
        who: selected(),
        amount: Value::Fixed(4),
    });
    assert_eq!(world.state.players[&P1].life, 23);
    assert_eq!(world.state.players[&P0].life, 20);
    let deltas: Vec<_> = world
        .log
        .iter()
        .filter_map(|entry| match entry.event {
            Event::LifeChanged { player, delta } => {
                assert_eq!(player, P1);
                Some(delta)
            }
            _ => None,
        })
        .collect();
    assert_eq!(deltas, vec![7, -4]);
    assert!(!world.log.iter().any(|entry| matches!(
        entry.event,
        Event::DamageDealtToPlayer { .. } | Event::DamageMarked { .. }
    )));
}

#[test]
fn cr_120_damage_to_player_loses_life_but_creature_damage_marks_without_lowering_toughness() {
    let mut world = World::new();
    let creature = world.place(1, P1, Zone::Battlefield);
    let damage = Effect::DealDamage {
        source: Selector::SelfSource,
        to: selected(),
        amount: Value::Fixed(3),
    };
    world.target(Target::Player(P1));
    world.run(damage.clone());
    assert_eq!(world.state.players[&P1].life, 17);
    world.target(Target::Object(creature));
    world.run(damage);
    assert_eq!(world.state.objects[&creature].damage, 3);
    assert_eq!(
        mtg_engine::layers::compute(&world.state, &world.cards, creature)
            .unwrap()
            .toughness,
        Some(5)
    );
    assert_eq!(world.state.players[&P1].life, 17);
    assert!(world.log.iter().any(|entry| matches!(entry.event,
        Event::DamageDealtToPlayer { player, amount: 3, counters: false, .. } if player == P1)));
    assert!(world.log.iter().any(|entry| matches!(entry.event,
        Event::DamageMarked { object, amount: 3, counters: false, .. } if object == creature)));
}

#[test]
fn cr_400_7_zone_changes_preserve_owner_reset_damage_and_use_shared_exile() {
    let mut world = World::new();
    let original = world.place(1, P1, Zone::Battlefield);
    let object = world.state.objects.get_mut(&original).unwrap();
    object.controller = P0;
    object.tapped = true;
    object.damage = 4;
    world.target(Target::Object(original));
    world.run(movement(selected(), Zone::Exile));
    let new = world.moved(original);
    assert_ne!(new, original);
    assert!(!world.state.objects.contains_key(&original));
    assert_eq!(world.state.objects[&new].zone, ZoneRef::shared(Zone::Exile));
    assert_eq!(world.state.objects[&new].owner, P1);
    assert_eq!(world.state.objects[&new].card, CardId(1));
    assert_eq!(world.state.objects[&new].damage, 0);
    assert!(!world.state.objects[&new].tapped);
    assert_eq!(world.state.objects.len(), 2);
}

#[test]
fn cr_121_4_failed_draw_loses_only_when_sbas_run_after_effect_finishes() {
    let mut world = World::new();
    world.place(1, P1, Zone::Library);
    world.place(2, P1, Zone::Library);
    world.target(Target::Player(P1));
    world.run(Effect::Sequence(vec![
        Effect::Draw {
            who: selected(),
            count: Value::Fixed(3),
        },
        Effect::GainLife {
            who: Selector::You,
            amount: Value::Fixed(4),
        },
    ]));
    assert_eq!(world.state.objects_in(ZoneRef::of(Zone::Hand, P1)).len(), 2);
    assert!(
        !world.state.players[&P1].has_lost,
        "draw failure cannot interrupt the resolving effect with an SBA"
    );
    assert_eq!(
        world.state.players[&P0].life, 24,
        "remaining instructions still resolve"
    );
    assert!(world.log.iter().any(|entry| matches!(entry.event,
        Event::AttemptedDrawFromEmptyLibrary { player } if player == P1)));
    let mut engine = mtg_engine::Engine::new(world.state);
    assert!(
        mtg_verify::scenario::next_choice(&mut engine, &world.cards)
            .unwrap()
            .is_none()
    );
    assert!(engine.state.players[&P1].has_lost);
    assert!(engine.log.iter().any(|entry| matches!(entry.event,
        Event::Lost { player, reason: mtg_core::LossReason::DrawFromEmptyLibrary } if player == P1)));
}

#[test]
fn cr_119_9_120_8_zero_life_gain_and_zero_damage_do_not_emit_events() {
    let mut world = World::new();
    world.target(Target::Player(P1));
    world.run(Effect::GainLife {
        who: selected(),
        amount: Value::Fixed(0),
    });
    world.run(Effect::DealDamage {
        source: Selector::SelfSource,
        to: selected(),
        amount: Value::Fixed(0),
    });
    assert_eq!(world.state.players[&P1].life, 20);
    assert!(
        world.log.is_empty(),
        "zero gain and damage are not life-gain or damage events"
    );
}

#[test]
fn cr_121_5_putting_library_cards_into_hand_without_drawing_is_not_a_draw() {
    let mut world = World::new();
    let original = world.place(1, P1, Zone::Library);
    world.target(Target::Player(P1));
    let put = movement(
        Selector::TopOfLibrary {
            player: Box::new(selected()),
            count: Value::Fixed(3),
        },
        Zone::Hand,
    );
    world.run(put.clone());
    world.run(put); // Empty library: this instruction still is not an attempted draw.
    assert_eq!(
        world.state.objects_in(ZoneRef::of(Zone::Hand, P1)),
        vec![world.moved(original)]
    );
    assert_eq!(world.state.objects.len(), 2);
    assert!(!world.log.iter().any(|entry| matches!(
        entry.event,
        Event::Drew { .. } | Event::AttemptedDrawFromEmptyLibrary { .. }
    )));
    let mut engine = mtg_engine::Engine::new(world.state);
    assert!(
        mtg_verify::scenario::next_choice(&mut engine, &world.cards)
            .unwrap()
            .is_some()
    );
    assert!(!engine.state.players[&P1].has_lost);
}
