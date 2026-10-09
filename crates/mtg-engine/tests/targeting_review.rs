//! Independent CR601.2c/608.2b adversarial review.
//! Official reference: https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf
mod common;

use common::{creature, creature_target, targeted_instant};
use mtg_core::{CardId, CardType, ObjectId, PlayerId, Step, Target, Zone, ZoneRef};
use mtg_engine::{Action, Answer, Choice, ChoiceKind, Engine, Progress, state::GameState};
use mtg_ir::{CardFace, Effect, ObjectFilter, PrintedCards, Selector, Value, selector::TargetSpec};

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const P2: PlayerId = PlayerId(2);
const CREATURE: CardId = CardId(0);
const SPELL: CardId = CardId(1);
struct Cards(Vec<CardFace>);
impl PrintedCards for Cards {
    fn face(&self, card: CardId, _: u8) -> Option<&CardFace> {
        self.0.get(card.0 as usize)
    }
    fn subtype_name(&self, _: mtg_core::Subtype) -> Option<&str> {
        None
    }
}
fn cards(specs: Vec<TargetSpec>, effect: Effect) -> Cards {
    Cards(vec![
        creature("Independent durable target", 4, 4, &[]),
        targeted_instant("Independent targeting spell", specs, effect),
    ])
}
fn state() -> GameState {
    let mut s = GameState::new(&[P0, P1, P2], 20);
    s.turn = 2;
    s.step = Step::PrecombatMain;
    s.active_player = P0;
    s
}
fn next(engine: &mut Engine, cards: &Cards) -> Choice {
    for _ in 0..1000 {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => return c,
            Progress::GameOver { .. } => panic!("unexpected game over"),
        }
    }
    panic!("bounded driver stalled")
}
fn cast(engine: &mut Engine, cards: &Cards, spell: ObjectId) -> Choice {
    let priority = next(engine, cards);
    assert_eq!(priority.who, P0);
    engine
        .answer(
            cards,
            priority.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let choice = next(engine, cards);
    assert!(matches!(choice.kind, ChoiceKind::ChooseTargets { .. }));
    choice
}
fn resolve(engine: &mut Engine, cards: &Cards) {
    for _ in 0..1000 {
        let c = next(engine, cards);
        if engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
        {
            return;
        }
        assert!(matches!(c.kind, ChoiceKind::Priority { .. }));
        engine.answer(cards, c.id, Answer::Pass).unwrap();
    }
    panic!("stack did not resolve")
}
fn heal() -> Effect {
    Effect::GainLife {
        who: Selector::Target { index: 0 },
        amount: Value::Fixed(3),
    }
}
fn player_spec(players: Option<Selector>) -> TargetSpec {
    TargetSpec {
        zone: Zone::Battlefield,
        filter: ObjectFilter::HasType(CardType::Land),
        allows_players: true,
        players,
        mode: None,
        count: Value::ONE,
        up_to: false,
        distinct_from_other_targets: false,
    }
}

#[test]
fn legal_distinct_assignment_is_offered_even_when_first_greedy_pick_blocks_later_slot() {
    let broad = creature_target();
    let narrow = TargetSpec {
        filter: ObjectFilter::And(vec![
            ObjectFilter::HasType(CardType::Creature),
            ObjectFilter::ControlledBy(Box::new(Selector::You)),
        ]),
        distinct_from_other_targets: true,
        ..creature_target()
    };
    let cards = cards(vec![broad, narrow], Effect::Nothing);
    let mut s = state();
    let own = s.place(CREATURE, P0, ZoneRef::shared(Zone::Battlefield));
    let other = s.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let spell = s.place(SPELL, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(s);
    let priority = next(&mut engine, &cards);
    let ChoiceKind::Priority { legal } = priority.kind else {
        panic!("priority expected")
    };
    assert!(
        legal.actions.contains(&Action::Cast { object: spell }),
        "enemy for broad slot, own for narrow slot is a legal CR601.2c assignment"
    );
    engine
        .answer(
            &cards,
            priority.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let first = next(&mut engine, &cards);
    engine
        .answer(
            &cards,
            first.id,
            Answer::Targets(vec![vec![Target::Object(other)]]),
        )
        .unwrap();
    let second = next(&mut engine, &cards);
    engine
        .answer(
            &cards,
            second.id,
            Answer::Targets(vec![vec![Target::Object(own)]]),
        )
        .unwrap();
    let _ = next(&mut engine, &cards);
    assert_eq!(
        engine.state.objects_in(ZoneRef::shared(Zone::Stack)).len(),
        1
    );
}

#[test]
fn changed_player_qualifier_makes_original_target_illegal_on_resolution() {
    let selector = Selector::ControllerOf(Box::new(Selector::All {
        zone: Zone::Battlefield,
        filter: ObjectFilter::HasType(CardType::Creature),
    }));
    let cards = cards(vec![player_spec(Some(selector))], heal());
    let mut s = state();
    let permanent = s.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let spell = s.place(SPELL, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(s);
    let c = cast(&mut engine, &cards, spell);
    engine
        .answer(
            &cards,
            c.id,
            Answer::Targets(vec![vec![Target::Player(P1)]]),
        )
        .unwrap();
    let _ = next(&mut engine, &cards);
    engine.state.objects.get_mut(&permanent).unwrap().controller = P2;
    engine.state.bump();
    resolve(&mut engine, &cards);
    assert_eq!(
        engine.state.player(P1).life,
        20,
        "P1 no longer controls the qualifying creature; sole target is illegal under CR608.2b"
    );
}

#[test]
fn nonadjacent_duplicate_targets_in_one_slot_are_rejected_without_replacing_choices() {
    let spec = TargetSpec {
        count: Value::Fixed(3),
        ..creature_target()
    };
    let cards = cards(vec![spec], Effect::Nothing);
    let mut s = state();
    let a = s.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let b = s.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let c = s.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let spell = s.place(SPELL, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(s);
    let question = cast(&mut engine, &cards, spell);
    let invalid = Answer::Targets(vec![vec![
        Target::Object(a),
        Target::Object(b),
        Target::Object(a),
    ]]);
    assert!(
        engine.answer(&cards, question.id, invalid).is_err(),
        "CR601.2c prohibits repeated target for one target occurrence"
    );
    let again = next(&mut engine, &cards);
    assert_eq!(again.id, question.id);
    engine
        .answer(
            &cards,
            again.id,
            Answer::Targets(vec![vec![
                Target::Object(a),
                Target::Object(b),
                Target::Object(c),
            ]]),
        )
        .unwrap();
}

#[test]
fn unknown_target_is_rejected_and_does_not_silently_choose_a_legal_one() {
    let cards = cards(vec![creature_target()], Effect::Nothing);
    let mut s = state();
    let creature = s.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let spell = s.place(SPELL, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(s);
    let question = cast(&mut engine, &cards, spell);
    assert!(
        engine
            .answer(
                &cards,
                question.id,
                Answer::Targets(vec![vec![Target::Object(ObjectId(u32::MAX))]])
            )
            .is_err()
    );
    let again = next(&mut engine, &cards);
    assert_eq!(again.id, question.id);
    engine
        .answer(
            &cards,
            again.id,
            Answer::Targets(vec![vec![Target::Object(creature)]]),
        )
        .unwrap();
}

#[test]
fn target_leaving_and_reentering_is_new_object_not_original_target() {
    use mtg_core::{Cause, Event};
    let damage = Effect::DealDamage {
        source: Selector::SelfSource,
        to: Selector::Target { index: 0 },
        amount: Value::Fixed(3),
    };
    let cards = cards(vec![creature_target()], damage);
    let mut s = state();
    let old = s.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let spell = s.place(SPELL, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(s);
    let question = cast(&mut engine, &cards, spell);
    engine
        .answer(
            &cards,
            question.id,
            Answer::Targets(vec![vec![Target::Object(old)]]),
        )
        .unwrap();
    let _ = next(&mut engine, &cards);
    let in_exile = engine.state.new_object_id();
    mtg_engine::apply::apply(
        &mut engine.state,
        Cause::PlayerAction(P1),
        Event::ZoneChange {
            object: old,
            new_object: in_exile,
            from: ZoneRef::shared(Zone::Battlefield),
            to: ZoneRef::shared(Zone::Exile),
            index: None,
        },
        &mut engine.log,
    );
    let returned = engine.state.new_object_id();
    mtg_engine::apply::apply(
        &mut engine.state,
        Cause::PlayerAction(P1),
        Event::ZoneChange {
            object: in_exile,
            new_object: returned,
            from: ZoneRef::shared(Zone::Exile),
            to: ZoneRef::shared(Zone::Battlefield),
            index: None,
        },
        &mut engine.log,
    );
    assert_ne!(old, returned);
    resolve(&mut engine, &cards);
    assert_eq!(engine.state.objects[&returned].damage, 0);
}

#[test]
fn departed_player_is_illegal_target_on_resolution() {
    let cards = cards(vec![player_spec(None)], heal());
    let mut s = state();
    let spell = s.place(SPELL, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(s);
    let question = cast(&mut engine, &cards, spell);
    engine
        .answer(
            &cards,
            question.id,
            Answer::Targets(vec![vec![Target::Player(P1)]]),
        )
        .unwrap();
    let p0 = next(&mut engine, &cards);
    engine.answer(&cards, p0.id, Answer::Pass).unwrap();
    let p1 = next(&mut engine, &cards);
    assert_eq!(p1.who, P1);
    engine
        .answer(&cards, p1.id, Answer::Action(Action::Concede))
        .unwrap();
    resolve(&mut engine, &cards);
    assert!(engine.state.player(P1).has_lost);
    assert_eq!(engine.state.player(P1).life, 20);
}

#[test]
fn another_creature_filter_excludes_permanent_source_rather_than_ability_stack_identity() {
    use mtg_ir::{Ability, AbilityKind, Cost, ability::ActivationTiming};
    let mut face = creature("Independent source distinction", 4, 4, &[]);
    face.abilities.push(Ability {
        id: mtg_core::AbilityId(0),
        source_text: None,
        targets: vec![TargetSpec {
            filter: ObjectFilter::And(vec![
                ObjectFilter::HasType(CardType::Creature),
                ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)),
            ]),
            ..creature_target()
        }],
        kind: AbilityKind::Activated {
            cost: Cost::free(),
            effect: Effect::Nothing,
            functions_from: Zone::Battlefield,
            is_mana_ability: false,
            is_loyalty_ability: false,
            timing: ActivationTiming::Instant,
        },
    });
    let cards = Cards(vec![face]);
    let mut s = state();
    let source = s.place(CREATURE, P0, ZoneRef::shared(Zone::Battlefield));
    let other = s.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(s);
    let p = next(&mut engine, &cards);
    engine
        .answer(
            &cards,
            p.id,
            Answer::Action(Action::ActivateAbility {
                source,
                ability: mtg_core::AbilityId(0),
            }),
        )
        .unwrap();
    let c = next(&mut engine, &cards);
    let ChoiceKind::ChooseTargets { slots, .. } = &c.kind else {
        panic!("target prompt expected")
    };
    assert_eq!(slots, &vec![vec![Target::Object(other)]]);
    assert!(
        !slots
            .iter()
            .flatten()
            .any(|target| *target == Target::Object(source))
    );
    engine
        .answer(
            &cards,
            c.id,
            Answer::Targets(vec![vec![Target::Object(other)]]),
        )
        .unwrap();
}

#[test]
fn multicreature_target_group_keeps_its_own_legality_spec_before_player_slot() {
    let pair = TargetSpec {
        count: Value::Fixed(2),
        ..creature_target()
    };
    let effect = Effect::Sequence(vec![
        Effect::Tap {
            what: Selector::Union(vec![
                Selector::Target { index: 0 },
                Selector::Target { index: 1 },
            ]),
        },
        Effect::GainLife {
            who: Selector::Target { index: 2 },
            amount: Value::Fixed(3),
        },
    ]);
    let cards = cards(vec![pair, player_spec(None)], effect);
    let mut s = state();
    let a = s.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let b = s.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let spell = s.place(SPELL, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(s);
    let first = cast(&mut engine, &cards, spell);
    engine
        .answer(
            &cards,
            first.id,
            Answer::Targets(vec![vec![Target::Object(a), Target::Object(b)]]),
        )
        .unwrap();
    let second = next(&mut engine, &cards);
    assert!(matches!(second.kind, ChoiceKind::ChooseTargets { .. }));
    engine
        .answer(
            &cards,
            second.id,
            Answer::Targets(vec![vec![Target::Player(P1)]]),
        )
        .unwrap();
    resolve(&mut engine, &cards);
    assert!(engine.state.objects[&a].tapped);
    assert!(
        engine.state.objects[&b].tapped,
        "second member of creature group must not be checked against later player slot"
    );
    assert_eq!(engine.state.player(P1).life, 23);
}

#[test]
fn same_object_can_be_chosen_once_for_each_separate_target_occurrence() {
    let effect = Effect::Sequence(vec![
        Effect::Tap {
            what: Selector::Target { index: 0 },
        },
        Effect::GainLife {
            who: Selector::You,
            amount: Value::Fixed(3),
        },
    ]);
    let cards = cards(vec![creature_target(), creature_target()], effect);
    let mut s = state();
    let object = s.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let spell = s.place(SPELL, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(s);
    let first = cast(&mut engine, &cards, spell);
    engine
        .answer(
            &cards,
            first.id,
            Answer::Targets(vec![vec![Target::Object(object)]]),
        )
        .unwrap();
    let second = next(&mut engine, &cards);
    engine
        .answer(
            &cards,
            second.id,
            Answer::Targets(vec![vec![Target::Object(object)]]),
        )
        .unwrap();
    resolve(&mut engine, &cards);
    assert!(engine.state.objects[&object].tapped);
    assert_eq!(engine.state.player(P0).life, 23);
}

#[test]
fn counter_spell_cannot_target_its_own_new_stack_object() {
    let old_face = common::instant("Independent old spell", mtg_core::ManaCost::FREE, 3);
    let spec = TargetSpec {
        zone: Zone::Stack,
        filter: ObjectFilter::HasType(CardType::Instant),
        ..creature_target()
    };
    let counter = targeted_instant(
        "Independent counter",
        vec![spec],
        Effect::CounterSpell {
            exile: false,
            what: Selector::Target { index: 0 },
        },
    );
    let cards = Cards(vec![
        creature("Independent creature", 4, 4, &[]),
        old_face,
        counter,
    ]);
    let mut s = state();
    let old = s.place(CardId(1), P1, ZoneRef::shared(Zone::Stack));
    s.objects.get_mut(&old).unwrap().cast_context = Some(Default::default());
    let new = s.place(CardId(2), P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(s);
    let c = cast(&mut engine, &cards, new);
    let ChoiceKind::ChooseTargets { slots, .. } = &c.kind else {
        panic!("targets expected")
    };
    assert_eq!(
        slots,
        &vec![vec![Target::Object(old)]],
        "CR115.5 excludes the casting spell itself on the stack"
    );
    engine
        .answer(
            &cards,
            c.id,
            Answer::Targets(vec![vec![Target::Object(old)]]),
        )
        .unwrap();
    resolve(&mut engine, &cards);
    assert_eq!(
        engine.state.player(P1).life,
        20,
        "countered spell never resolves"
    );
}
