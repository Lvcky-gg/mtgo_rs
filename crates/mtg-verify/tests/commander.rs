mod support;

use mtg_core::{CardId, Event, ManaPool, Supertype, Zone, ZoneRef};
use mtg_engine::{Engine, actions::Action, choice::ChoiceKind};
use support::*;

fn cards() -> Cards {
    let mut cards = Cards::creature(2, false);
    cards.0.supertypes.push(Supertype::Legendary);
    cards
}

fn offered(choice: &mtg_engine::Choice, object: mtg_core::ObjectId) -> bool {
    matches!(&choice.kind, ChoiceKind::Priority { legal }
        if legal.actions.iter().any(|action| matches!(action, Action::Cast { object: id } if *id == object)))
}

#[test]
fn cr_903_8_shared_command_zone_casting_is_owner_only() {
    let mut state = main_state();
    state
        .commander
        .commanders
        .extend([(P0, CREATURE), (P1, CREATURE)]);
    let own = state.place(CREATURE, P0, ZoneRef::shared(Zone::Command));
    let opponent = state.place(CREATURE, P1, ZoneRef::shared(Zone::Command));
    let mut engine = Engine::new(state);
    let choice = mtg_verify::scenario::next_choice(&mut engine, &cards())
        .unwrap()
        .unwrap();
    assert!(
        offered(&choice, own),
        "own commander must be castable from shared command zone"
    );
    assert!(
        !offered(&choice, opponent),
        "opponent commander must not be castable"
    );
    assert!(
        engine
            .answer(
                &cards(),
                choice.id,
                mtg_engine::choice::Answer::Action(Action::Cast { object: opponent })
            )
            .is_err(),
        "forged opponent-command casting action must be rejected"
    );
}

#[test]
fn cr_903_8_tax_requires_two_mana_for_each_previous_command_zone_cast() {
    for previous in 0..=3 {
        for mana in 0..=7 {
            let mut state = main_state();
            state.commander.commanders.insert(P0, CREATURE);
            state.commander.casts.insert(P0, previous);
            state.players.get_mut(&P0).unwrap().mana.amounts[ManaPool::COLORLESS_SLOT] = mana;
            let commander = state.place(CREATURE, P0, ZoneRef::shared(Zone::Command));
            let mut engine = Engine::new(state);
            let choice = mtg_verify::scenario::next_choice(&mut engine, &cards())
                .unwrap()
                .unwrap();
            assert_eq!(
                offered(&choice, commander),
                u32::from(mana) >= 2 * previous,
                "previous casts={previous}, available mana={mana}"
            );
        }
    }
}

#[test]
fn commander_return_destination_uses_shared_command_zone() {
    use mtg_engine::choice::Answer;
    let mut state = main_state();
    state.commander.commanders.insert(P0, CREATURE);
    let original = state.place(CREATURE, P0, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&original).unwrap().damage = 2;
    let mut engine = Engine::new(state);
    let cards = cards();
    let choice = mtg_verify::scenario::next_choice(&mut engine, &cards)
        .unwrap()
        .unwrap();
    assert!(matches!(choice.kind, ChoiceKind::Confirm));
    assert_eq!(choice.who, P0);
    engine
        .answer(&cards, choice.id, Answer::Bool(true))
        .unwrap();
    stabilize(&mut engine, &cards);
    let commanders: Vec<_> = engine
        .state
        .objects
        .values()
        .filter(|object| object.card == CardId(0))
        .collect();
    assert_eq!(commanders.len(), 1);
    assert_eq!(commanders[0].zone, ZoneRef::shared(Zone::Command));
    assert_eq!(commanders[0].owner, P0);
    assert!(engine.log.iter().any(|entry| matches!(entry.event,
        Event::ZoneChange { to, .. } if to == ZoneRef::shared(Zone::Command))));
}

fn entered_commander(zone: Zone) -> (Engine, mtg_core::ObjectId) {
    let mut state = main_state();
    state.commander.commanders.insert(P1, CREATURE);
    let old = state.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&old).unwrap().controller = P0;
    let new = state.new_object_id();
    let to = if zone.is_shared() {
        ZoneRef::shared(zone)
    } else {
        ZoneRef::of(zone, P1)
    };
    let mut log = vec![];
    mtg_engine::apply::apply(
        &mut state,
        mtg_core::Cause::Resolution(old),
        Event::ZoneChange {
            object: old,
            new_object: new,
            from: ZoneRef::shared(Zone::Battlefield),
            to,
            index: None,
        },
        &mut log,
    );
    (Engine::new(state), new)
}

#[test]
fn cr_903_9a_owner_can_accept_or_decline_graveyard_and_exile_returns() {
    use mtg_engine::choice::Answer;
    for zone in [Zone::Graveyard, Zone::Exile] {
        for accept in [false, true] {
            let (mut engine, original) = entered_commander(zone);
            let cards = cards();
            let choice = mtg_verify::scenario::next_choice(&mut engine, &cards)
                .unwrap()
                .unwrap();
            assert!(
                matches!(choice.kind, ChoiceKind::Confirm),
                "owner must receive an actual decision"
            );
            assert_eq!(
                choice.who, P1,
                "former controller cannot make the owner's choice"
            );
            engine
                .answer(&cards, choice.id, Answer::Bool(accept))
                .unwrap();
            let next = mtg_verify::scenario::next_choice(&mut engine, &cards)
                .unwrap()
                .unwrap();
            assert!(matches!(next.kind, ChoiceKind::Priority { .. }));
            let object = engine
                .state
                .objects
                .values()
                .find(|o| o.card == CREATURE)
                .unwrap();
            assert_eq!(object.zone.zone, if accept { Zone::Command } else { zone });
            assert_eq!(object.owner, P1);
            if accept {
                assert_ne!(object.id, original);
                assert_eq!(object.zone, ZoneRef::shared(Zone::Command));
            } else {
                assert_eq!(object.id, original);
                engine.answer(&cards, next.id, Answer::Pass).unwrap();
                let next = mtg_verify::scenario::next_choice(&mut engine, &cards)
                    .unwrap()
                    .unwrap();
                assert!(
                    matches!(next.kind, ChoiceKind::Priority { .. }),
                    "declined object must not be offered repeatedly"
                );
            }
        }
    }
}

#[test]
fn cr_903_9a_reentering_after_decline_creates_a_fresh_return_option() {
    use mtg_engine::choice::Answer;
    let (mut engine, first) = entered_commander(Zone::Graveyard);
    let cards = cards();
    let choice = mtg_verify::scenario::next_choice(&mut engine, &cards)
        .unwrap()
        .unwrap();
    assert!(matches!(choice.kind, ChoiceKind::Confirm));
    engine
        .answer(&cards, choice.id, Answer::Bool(false))
        .unwrap();
    let graveyard = ZoneRef::of(Zone::Graveyard, P1);
    let battlefield = ZoneRef::shared(Zone::Battlefield);
    let second = engine.state.new_object_id();
    mtg_engine::apply::apply(
        &mut engine.state,
        mtg_core::Cause::Resolution(first),
        Event::ZoneChange {
            object: first,
            new_object: second,
            from: graveyard,
            to: battlefield,
            index: None,
        },
        &mut engine.log,
    );
    let third = engine.state.new_object_id();
    mtg_engine::apply::apply(
        &mut engine.state,
        mtg_core::Cause::Resolution(second),
        Event::ZoneChange {
            object: second,
            new_object: third,
            from: battlefield,
            to: graveyard,
            index: None,
        },
        &mut engine.log,
    );
    let choice = mtg_verify::scenario::next_choice(&mut engine, &cards)
        .unwrap()
        .unwrap();
    assert!(
        matches!(choice.kind, ChoiceKind::Confirm),
        "new zone identity must regain the option"
    );
    assert_eq!(choice.who, P1);
    engine
        .answer(&cards, choice.id, Answer::Bool(true))
        .unwrap();
    stabilize(&mut engine, &cards);
    assert_eq!(
        engine.state.objects.values().next().unwrap().zone,
        ZoneRef::shared(Zone::Command)
    );
}

#[test]
fn cr_704_mandatory_deaths_settle_before_priority_after_return_decision() {
    use mtg_engine::choice::Answer;
    let (mut engine, _) = entered_commander(Zone::Exile);
    let ordinary = engine
        .state
        .place(CREATURE, P0, ZoneRef::shared(Zone::Battlefield));
    engine.state.objects.get_mut(&ordinary).unwrap().damage = 2;
    let cards = cards();
    let choice = mtg_verify::scenario::next_choice(&mut engine, &cards)
        .unwrap()
        .unwrap();
    assert!(matches!(choice.kind, ChoiceKind::Confirm));
    engine
        .answer(&cards, choice.id, Answer::Bool(false))
        .unwrap();
    let priority = mtg_verify::scenario::next_choice(&mut engine, &cards)
        .unwrap()
        .unwrap();
    assert!(matches!(priority.kind, ChoiceKind::Priority { .. }));
    assert!(!engine.state.objects.contains_key(&ordinary));
    assert_eq!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .len(),
        1
    );
    assert!(engine.state.battlefield().is_empty());
}

#[test]
fn cr_704_903_optional_returns_use_apnap_decisions_and_simultaneous_moves() {
    use mtg_engine::choice::Answer;
    let mut state = main_state();
    state.active_player = P1;
    state.priority = Some(P1);
    state
        .commander
        .commanders
        .extend([(P0, CREATURE), (P1, CREATURE)]);
    state.place(CREATURE, P0, ZoneRef::of(Zone::Graveyard, P0));
    state.place(CREATURE, P1, ZoneRef::of(Zone::Graveyard, P1));
    let mut engine = Engine::new(state);
    let cards = cards();
    let first = mtg_verify::scenario::next_choice(&mut engine, &cards)
        .unwrap()
        .unwrap();
    assert!(matches!(first.kind, ChoiceKind::Confirm));
    assert_eq!(
        first.who, P1,
        "active player makes simultaneous decisions first"
    );
    assert!(engine.answer(&cards, first.id, Answer::Pass).is_err());
    let repeated = mtg_verify::scenario::next_choice(&mut engine, &cards)
        .unwrap()
        .unwrap();
    assert_eq!(
        repeated.id, first.id,
        "wrong answer must preserve pending decision"
    );
    assert_eq!(repeated.who, P1);
    engine
        .answer(&cards, repeated.id, Answer::Bool(true))
        .unwrap();
    let second = mtg_verify::scenario::next_choice(&mut engine, &cards)
        .unwrap()
        .unwrap();
    assert!(matches!(second.kind, ChoiceKind::Confirm));
    assert_eq!(second.who, P0);
    assert!(
        engine
            .state
            .objects
            .values()
            .all(|object| object.zone.zone == Zone::Graveyard),
        "accepted first move must wait for the rest of the simultaneous decisions"
    );
    engine
        .answer(&cards, second.id, Answer::Bool(true))
        .unwrap();
    let priority = mtg_verify::scenario::next_choice(&mut engine, &cards)
        .unwrap()
        .unwrap();
    assert!(matches!(priority.kind, ChoiceKind::Priority { .. }));
    assert!(
        engine
            .state
            .objects
            .values()
            .all(|object| object.zone == ZoneRef::shared(Zone::Command))
    );
    let moves: Vec<_> = engine
        .log
        .iter()
        .filter(|entry| {
            matches!(entry.event,
        Event::ZoneChange { to, .. } if to == ZoneRef::shared(Zone::Command))
        })
        .collect();
    assert_eq!(moves.len(), 2);
    assert_eq!(
        moves[0].at, moves[1].at,
        "both SBA moves must occur simultaneously"
    );
}

#[test]
fn cr_903_8_second_command_zone_cast_spends_tax_and_advances_cast_count() {
    use mtg_engine::choice::Answer;
    let mut state = main_state();
    state.commander.commanders.insert(P0, CREATURE);
    state.commander.casts.insert(P0, 1);
    state.players.get_mut(&P0).unwrap().mana.amounts[ManaPool::COLORLESS_SLOT] = 2;
    let original = state.place(CREATURE, P0, ZoneRef::shared(Zone::Command));
    let mut engine = Engine::new(state);
    let cards = cards();
    let choice = mtg_verify::scenario::next_choice(&mut engine, &cards)
        .unwrap()
        .unwrap();
    assert!(offered(&choice, original));
    engine
        .answer(
            &cards,
            choice.id,
            Answer::Action(Action::Cast { object: original }),
        )
        .unwrap();
    for _ in 0..32 {
        let choice = mtg_verify::scenario::next_choice(&mut engine, &cards)
            .unwrap()
            .unwrap();
        if !engine.state.battlefield().is_empty() {
            break;
        }
        let answer = if matches!(choice.kind, ChoiceKind::Priority { .. }) {
            Answer::Pass
        } else {
            mtg_policy::well_formed(&choice, &engine.view_for(choice.who))
        };
        engine.answer(&cards, choice.id, answer).unwrap();
    }
    assert_eq!(engine.state.battlefield().len(), 1);
    assert_eq!(engine.state.players[&P0].mana.total(), 0);
    assert_eq!(engine.state.commander.casts[&P0], 2);
    assert!(!engine.state.objects.contains_key(&original));
}
