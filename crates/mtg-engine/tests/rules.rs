//! State-based actions, the layer system, and the evaluator.

mod common;

use common::*;
use mtg_core::{CounterKind, ObjectId, Step, Zone, ZoneRef};
use mtg_engine::{
    Engine,
    layers::{self, layer},
    state::{AffectedSet, ContinuousEffect, GameState},
};
use mtg_ir::{ObjectFilter, Selector, Value, effect::Duration, effect::Modification};

fn game() -> GameState {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);
    // Enough library that nobody loses to an empty draw mid-test.
    for _ in 0..10 {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Library, P0));
        state.place(DUMMY, P1, ZoneRef::of(Zone::Library, P1));
    }
    state
}

fn in_graveyard(engine: &Engine, owner: mtg_core::PlayerId) -> usize {
    engine
        .state
        .objects_in(ZoneRef::of(Zone::Graveyard, owner))
        .len()
}

// ---- state-based actions ------------------------------------------------

#[test]
fn opposing_power_toughness_counters_cancel_before_priority() {
    use mtg_engine::{Progress, apply};
    let mut state = game();
    state.step = Step::PrecombatMain;
    let creature = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    let mut log = Vec::new();
    for (kind, delta) in [
        (CounterKind::PlusOnePlusOne, 3),
        (CounterKind::MinusOneMinusOne, 2),
    ] {
        apply::apply(
            &mut state,
            mtg_core::Cause::TurnStructure,
            mtg_core::Event::CountersChanged {
                object: creature,
                kind,
                delta,
            },
            &mut log,
        );
    }
    assert_eq!(
        state.objects[&creature].counters.len(),
        2,
        "cancellation is an SBA, not event application"
    );
    let mut engine = Engine::new(state);
    for _ in 0..200 {
        if let Progress::NeedsChoice(_) = engine.advance(&TestCards::default()) {
            let object = &engine.state.objects[&creature];
            assert_eq!(object.counters.get(&CounterKind::PlusOnePlusOne), Some(&1));
            assert!(!object.counters.contains_key(&CounterKind::MinusOneMinusOne));
            return;
        }
    }
    panic!("never reached priority");
}

#[test]
fn death_and_counter_cancellation_preserve_pre_sba_last_known_counters() {
    use mtg_engine::Progress;
    let mut state = game();
    let creature = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&creature).unwrap().counters.extend([
        (CounterKind::PlusOnePlusOne, 1),
        (CounterKind::MinusOneMinusOne, 3),
    ]);
    let mut engine = Engine::new(state);
    for _ in 0..200 {
        if let Progress::NeedsChoice(_) = engine.advance(&TestCards::default()) {
            assert!(!engine.state.objects.contains_key(&creature));
            let previous = &engine.state.last_known[&creature];
            assert_eq!(
                previous.counters.get(&CounterKind::PlusOnePlusOne),
                Some(&1)
            );
            assert_eq!(
                previous.counters.get(&CounterKind::MinusOneMinusOne),
                Some(&3)
            );
            return;
        }
    }
    panic!("never reached priority");
}

#[test]
fn cleanup_state_based_action_without_a_trigger_grants_priority_and_repeats() {
    use mtg_engine::{
        Progress,
        choice::{Answer, ChoiceKind},
    };
    let mut state = game();
    state.step = Step::Cleanup;
    let creature = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    state
        .objects
        .get_mut(&creature)
        .unwrap()
        .counters
        .insert(CounterKind::MinusOneMinusOne, 2);
    pump(&mut state, creature, 0, 1, layer::PT_MODIFY);
    let cards = TestCards::default();
    assert_eq!(
        layers::compute(&state, &cards, creature).unwrap().toughness,
        Some(1)
    );
    let mut engine = Engine::new(state);
    let mut priority = false;
    for _ in 0..200 {
        if engine.state.active_player == P1 {
            assert!(
                priority,
                "an SBA during cleanup grants priority even without triggers"
            );
            assert!(!engine.state.objects.contains_key(&creature));
            assert_eq!(
                engine
                    .log
                    .iter()
                    .filter(|event| matches!(
                        event.event,
                        mtg_core::Event::StepBegan {
                            step: Step::Cleanup,
                            ..
                        }
                    ))
                    .count(),
                2
            );
            return;
        }
        if let Progress::NeedsChoice(question) = engine.advance(&cards) {
            assert!(matches!(question.kind, ChoiceKind::Priority { .. }));
            assert_eq!(engine.state.step, Step::Cleanup);
            priority = true;
            engine.answer(&cards, question.id, Answer::Pass).unwrap();
        }
    }
    panic!("cleanup did not finish");
}

#[test]
fn ordinary_cleanup_has_no_priority_window_or_extra_cleanup() {
    use mtg_engine::Progress;
    let mut state = game();
    state.step = Step::Cleanup;
    let mut engine = Engine::new(state);
    for _ in 0..200 {
        if let Progress::NeedsChoice(question) = engine.advance(&TestCards::default()) {
            assert_eq!(question.who, P1);
            assert_eq!(engine.state.step, Step::Upkeep);
            assert_eq!(
                engine
                    .log
                    .iter()
                    .filter(|event| matches!(
                        event.event,
                        mtg_core::Event::StepBegan {
                            step: Step::Cleanup,
                            ..
                        }
                    ))
                    .count(),
                1
            );
            return;
        }
    }
    panic!("next turn never received priority");
}

#[test]
fn a_player_at_zero_life_loses() {
    let mut state = game();
    state.players.get_mut(&P1).unwrap().life = 0;
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run(&mut engine, &cards, 500, |e| e.state.player(P1).has_lost);
    assert!(engine.state.player(P1).has_lost);
}

#[test]
fn a_creature_with_lethal_damage_is_destroyed() {
    let mut state = game();
    let c = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    // A 2/2 with 2 damage marked.
    state.objects.get_mut(&c).unwrap().damage = 2;
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run(&mut engine, &cards, 500, |e| {
        !e.state.objects.contains_key(&c)
    });

    assert!(
        !engine.state.objects.contains_key(&c),
        "it should have left the battlefield"
    );
    assert_eq!(
        in_graveyard(&engine, P0),
        1,
        "and be in its owner's graveyard"
    );
}

#[test]
fn damage_below_toughness_is_not_lethal() {
    let mut state = game();
    let c = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&c).unwrap().damage = 1;
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run(&mut engine, &cards, 300, |e| e.state.step == Step::End);
    assert!(
        engine.state.objects.contains_key(&c),
        "a 2/2 with 1 damage survives"
    );
}

#[test]
fn any_amount_of_deathtouch_damage_is_lethal() {
    // CR 704.5h. One damage from a deathtouch source kills a 6/6.
    let mut state = game();
    let victim = state.place(BIG, P0, ZoneRef::shared(Zone::Battlefield));
    {
        let v = state.objects.get_mut(&victim).unwrap();
        v.damage = 1;
        v.dealt_deathtouch_damage = true;
    }
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run(&mut engine, &cards, 500, |e| {
        !e.state.objects.contains_key(&victim)
    });
    assert!(
        !engine.state.objects.contains_key(&victim),
        "one deathtouch damage should be lethal to a 6/6"
    );
}

#[test]
fn zero_toughness_puts_a_creature_in_the_graveyard_rather_than_destroying_it() {
    // CR 704.5f is a graveyard move, not destruction — which is why
    // indestructible does not save a creature whose toughness hits zero. Here a
    // -1/-1 counter on a 1/3... is not enough, so use three of them.
    let mut state = game();
    let c = state.place(SENTRY, P0, ZoneRef::shared(Zone::Battlefield));
    state
        .objects
        .get_mut(&c)
        .unwrap()
        .counters
        .insert(CounterKind::MinusOneMinusOne, 3);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run(&mut engine, &cards, 500, |e| {
        !e.state.objects.contains_key(&c)
    });
    assert!(
        !engine.state.objects.contains_key(&c),
        "toughness 0 means it dies"
    );
}

#[test]
fn the_legend_rule_asks_its_controller_which_one_to_keep() {
    let mut state = game();
    let a = state.place(LEGEND, P0, ZoneRef::shared(Zone::Battlefield));
    let b = state.place(LEGEND, P0, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let interruptions = run(&mut engine, &cards, 200, |e| {
        !(e.state.objects.contains_key(&a) && e.state.objects.contains_key(&b))
    });

    assert!(
        interruptions
            .iter()
            .any(|c| matches!(c.kind, mtg_engine::ChoiceKind::KeepOneLegend { .. })),
        "the controller should be asked which legend to keep, got {interruptions:#?}"
    );
}

#[test]
fn two_players_may_each_control_a_legend_with_the_same_name() {
    // The legend rule is per-controller, not global.
    let mut state = game();
    let a = state.place(LEGEND, P0, ZoneRef::shared(Zone::Battlefield));
    let b = state.place(LEGEND, P1, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run(&mut engine, &cards, 300, |e| e.state.step == Step::End);
    assert!(engine.state.objects.contains_key(&a));
    assert!(engine.state.objects.contains_key(&b));
}

#[test]
fn a_token_that_leaves_the_battlefield_ceases_to_exist() {
    let mut state = game();
    let t = state.place(DUMMY, P0, ZoneRef::of(Zone::Graveyard, P0));
    state.objects.get_mut(&t).unwrap().is_token = true;
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    run(&mut engine, &cards, 300, |e| {
        e.state
            .objects
            .get(&t)
            .is_none_or(|o| o.zone.zone != Zone::Graveyard)
    });
    assert!(
        engine
            .state
            .objects
            .get(&t)
            .is_none_or(|o| o.zone.zone != Zone::Graveyard),
        "a token outside the battlefield should not persist in the graveyard"
    );
}

// ---- the layer system ---------------------------------------------------

fn pump(state: &mut GameState, target: ObjectId, dp: i32, dt: i32, lay: u8) {
    let id = state.new_object_id();
    let timestamp = state.bump();
    state.continuous.push(ContinuousEffect {
        id,
        source: target,
        affected: AffectedSet::Fixed(vec![target]),
        modification: if lay == layer::PT_SWITCH {
            Modification::SwitchPowerToughness
        } else if lay == layer::PT_SET {
            Modification::SetBasePowerToughness {
                power: Value::Fixed(dp),
                toughness: Value::Fixed(dt),
            }
        } else {
            Modification::ModifyPowerToughness {
                power: Value::Fixed(dp),
                toughness: Value::Fixed(dt),
            }
        },
        duration: Duration::UntilEndOfTurn,
        timestamp,
        layer: lay,
        ability: None,
        controller: None,
    });
}

#[test]
fn a_pump_effect_changes_power_through_the_layer_system() {
    let mut state = game();
    let c = state.place(SENTRY, P0, ZoneRef::shared(Zone::Battlefield));
    pump(&mut state, c, 2, 0, layer::PT_MODIFY);
    let cards = TestCards::default();

    let ch = layers::compute(&state, &cards, c).expect("characteristics");
    assert_eq!(
        (ch.power, ch.toughness),
        (Some(3), Some(3)),
        "1/3 with +2/+0 is 3/3"
    );
}

#[test]
fn counters_are_applied_as_part_of_layer_seven() {
    let mut state = game();
    let c = state.place(SENTRY, P0, ZoneRef::shared(Zone::Battlefield));
    state
        .objects
        .get_mut(&c)
        .unwrap()
        .counters
        .insert(CounterKind::PlusOnePlusOne, 2);
    let cards = TestCards::default();

    let ch = layers::compute(&state, &cards, c).expect("characteristics");
    assert_eq!(
        (ch.power, ch.toughness),
        (Some(3), Some(5)),
        "1/3 with two +1/+1 is 3/5"
    );
}

#[test]
fn switching_power_and_toughness_happens_after_modifications() {
    // The ordering that matters: 7c (modify) then 7e (switch).
    //   correct: 1/3 --+2/+0--> 3/3 --switch--> 3/3
    //   wrong:   1/3 --switch--> 3/1 --+2/+0--> 5/1
    let mut state = game();
    let c = state.place(SENTRY, P0, ZoneRef::shared(Zone::Battlefield));
    pump(&mut state, c, 2, 0, layer::PT_MODIFY);
    pump(&mut state, c, 0, 0, layer::PT_SWITCH);
    let cards = TestCards::default();

    let ch = layers::compute(&state, &cards, c).expect("characteristics");
    assert_eq!(
        (ch.power, ch.toughness),
        (Some(3), Some(3)),
        "the switch must see the post-modification values"
    );
}

#[test]
fn setting_base_power_is_overridden_by_later_modifications() {
    // 7b sets, 7c modifies on top of it.
    let mut state = game();
    let c = state.place(SENTRY, P0, ZoneRef::shared(Zone::Battlefield));
    pump(&mut state, c, 4, 4, layer::PT_SET);
    pump(&mut state, c, 1, 1, layer::PT_MODIFY);
    let cards = TestCards::default();

    let ch = layers::compute(&state, &cards, c).expect("characteristics");
    assert_eq!(
        (ch.power, ch.toughness),
        (Some(5), Some(5)),
        "set 4/4 then +1/+1 is 5/5"
    );
}

#[test]
fn a_dynamic_anthem_applies_to_whatever_currently_matches() {
    // A static ability re-evaluates its affected set on every layer pass, so it
    // catches a creature that was not there when it resolved.
    let mut state = game();
    let source = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    let id = state.new_object_id();
    let timestamp = state.bump();
    state.continuous.push(ContinuousEffect {
        id,
        source,
        affected: AffectedSet::Dynamic(Selector::All {
            zone: Zone::Battlefield,
            filter: ObjectFilter::HasType(mtg_core::CardType::Creature),
        }),
        modification: Modification::ModifyPowerToughness {
            power: Value::ONE,
            toughness: Value::ONE,
        },
        duration: Duration::WhileSourcePresent,
        timestamp,
        layer: layer::PT_MODIFY,
        ability: None,
        controller: None,
    });

    // Added after the effect already existed.
    let latecomer = state.place(SENTRY, P0, ZoneRef::shared(Zone::Battlefield));
    let cards = TestCards::default();

    let ch = layers::compute(&state, &cards, latecomer).expect("characteristics");
    assert_eq!(
        (ch.power, ch.toughness),
        (Some(2), Some(4)),
        "a dynamic anthem should pick up a creature that arrived later"
    );
}

// ---- the evaluator -----------------------------------------------------

#[test]
fn counting_a_selector_reflects_the_board() {
    use mtg_engine::eval::{self, ComputedChars, Ctx};

    let mut state = game();
    let a = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(SENTRY, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(FIELD, P0, ZoneRef::shared(Zone::Battlefield));
    let cards = TestCards::default();
    let chars = ComputedChars(&cards);

    let ctx = Ctx {
        state: &state,
        cards: &cards,
        chars: &chars,
        source: a,
        controller: P0,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: &Default::default(),
    };

    let creatures = Value::Count(Box::new(Selector::All {
        zone: Zone::Battlefield,
        filter: ObjectFilter::HasType(mtg_core::CardType::Creature),
    }));
    assert_eq!(
        eval::value(&ctx, &creatures).unwrap(),
        2,
        "two creatures, not the land"
    );

    let lands = Value::Count(Box::new(Selector::All {
        zone: Zone::Battlefield,
        filter: ObjectFilter::HasType(mtg_core::CardType::Land),
    }));
    assert_eq!(eval::value(&ctx, &lands).unwrap(), 1);
}

#[test]
fn filters_compose_with_and_or_and_not() {
    use mtg_core::CardType;
    use mtg_engine::eval::{self, ComputedChars, Ctx};

    let mut state = game();
    let creature = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    let land = state.place(FIELD, P0, ZoneRef::shared(Zone::Battlefield));
    let cards = TestCards::default();
    let chars = ComputedChars(&cards);
    let ctx = Ctx {
        state: &state,
        cards: &cards,
        chars: &chars,
        source: creature,
        controller: P0,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: &Default::default(),
    };

    let not_a_land = ObjectFilter::Not(Box::new(ObjectFilter::HasType(CardType::Land)));
    assert!(eval::matches(&ctx, &not_a_land, creature).unwrap());
    assert!(!eval::matches(&ctx, &not_a_land, land).unwrap());

    let either = ObjectFilter::Or(vec![
        ObjectFilter::HasType(CardType::Land),
        ObjectFilter::HasType(CardType::Creature),
    ]);
    assert!(eval::matches(&ctx, &either, creature).unwrap());
    assert!(eval::matches(&ctx, &either, land).unwrap());

    let both = ObjectFilter::And(vec![
        ObjectFilter::HasType(CardType::Creature),
        ObjectFilter::HasType(CardType::Land),
    ]);
    assert!(!eval::matches(&ctx, &both, creature).unwrap());
}

#[test]
fn power_filters_read_through_the_layer_system_not_the_printed_card() {
    use mtg_engine::eval::{self, ComputedChars, Ctx};

    let mut state = game();
    // A 1/3 pumped to 3/3 should satisfy "power 3 or greater".
    let c = state.place(SENTRY, P0, ZoneRef::shared(Zone::Battlefield));
    pump(&mut state, c, 2, 0, layer::PT_MODIFY);
    let cards = TestCards::default();
    let chars = ComputedChars(&cards);
    let ctx = Ctx {
        state: &state,
        cards: &cards,
        chars: &chars,
        source: c,
        controller: P0,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: &Default::default(),
    };

    assert!(
        eval::matches(&ctx, &ObjectFilter::PowerAtLeast(Value::Fixed(3)), c).unwrap(),
        "the filter must see the pumped power"
    );
    assert!(!eval::matches(&ctx, &ObjectFilter::PowerAtLeast(Value::Fixed(4)), c).unwrap());
}
