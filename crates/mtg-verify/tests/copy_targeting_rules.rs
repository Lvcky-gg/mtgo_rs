mod support;

use mtg_core::{AbilityId, CardId, CardType, Event, ManaSymbol, ObjectId, Target, Zone, ZoneRef};
use mtg_engine::{
    Engine,
    actions::Action,
    choice::{Answer, ChoiceKind},
    layers::PrintedCards,
};
use mtg_ir::{
    Ability, AbilityKind, CardFace, Effect, ObjectFilter, Selector, Value, selector::TargetSpec,
};
use support::*;

struct Cards(Vec<CardFace>);
impl PrintedCards for Cards {
    fn face(&self, card: CardId, _face: u8) -> Option<&CardFace> {
        self.0.get(card.0 as usize)
    }
    fn subtype_name(&self, _: mtg_core::Subtype) -> Option<&str> {
        None
    }
}
fn target(zone: Zone, count: i32, optional: bool) -> TargetSpec {
    TargetSpec {
        zone,
        filter: ObjectFilter::HasType(if zone == Zone::Stack {
            CardType::Instant
        } else {
            CardType::Creature
        }),
        allows_players: false,
        players: None,
        mode: None,
        count: Value::Fixed(count),
        up_to: optional,
        distinct_from_other_targets: false,
    }
}
fn spell(name: &str, targets: Vec<TargetSpec>, effect: Effect) -> CardFace {
    let mut card = support::Cards::creature(10, false).0;
    card.name = name.into();
    card.card_types = vec![CardType::Instant];
    card.power = None;
    card.toughness = None;
    card.abilities = vec![Ability {
        id: AbilityId(0),
        kind: AbilityKind::SpellEffect(effect),
        targets,
        source_text: None,
    }];
    card
}
struct Table {
    engine: Engine,
    cards: Cards,
    creatures: [ObjectId; 3],
    original: ObjectId,
    copier: ObjectId,
    response: ObjectId,
}
impl Table {
    fn new(mixed: bool, modal: bool, retarget: bool) -> Self {
        let specs = if mixed {
            vec![
                target(Zone::Battlefield, 2, false),
                target(Zone::Battlefield, 1, true),
            ]
        } else {
            vec![target(Zone::Battlefield, 1, false)]
        };
        let effect = Effect::Sequence(vec![
            Effect::DealDamage {
                source: Selector::SelfSource,
                to: Selector::Target { index: 0 },
                amount: if modal { Value::X } else { Value::Fixed(2) },
            },
            Effect::GainLife {
                who: Selector::You,
                amount: Value::Fixed(1),
            },
        ]);
        let mut original = spell(
            "Independent copied spell",
            specs,
            if modal {
                Effect::Modal {
                    choose: Value::ONE,
                    modes: vec![
                        ("damage".into(), effect),
                        (
                            "different life".into(),
                            Effect::GainLife {
                                who: Selector::You,
                                amount: Value::Fixed(9),
                            },
                        ),
                    ],
                    at_least: None,
                }
            } else {
                effect
            },
        );
        if modal {
            original.mana_cost.symbols = vec![ManaSymbol::Variable];
            original.abilities[0].targets[0].mode = Some(0);
        }
        let copier = spell(
            "Independent copy effect",
            vec![target(Zone::Stack, 1, false)],
            Effect::CopySpell {
                what: Selector::Target { index: 0 },
                may_change_targets: retarget,
            },
        );
        let mut state = main_state();
        state.players.get_mut(&P0).unwrap().mana.amounts[5] = 3;
        let creatures =
            std::array::from_fn(|_| state.place(CardId(0), P1, ZoneRef::shared(Zone::Battlefield)));
        let original_id = state.place(CardId(1), P0, ZoneRef::of(Zone::Hand, P0));
        let copier_id = state.place(CardId(2), P1, ZoneRef::of(Zone::Hand, P1));
        let response_id = state.place(CardId(3), P1, ZoneRef::of(Zone::Hand, P1));
        let response = spell(
            "Independent exile response",
            vec![target(Zone::Battlefield, 1, false)],
            Effect::MoveZone {
                what: Selector::Target { index: 0 },
                to: Zone::Exile,
                owner_relative_to: None,
                position: mtg_ir::effect::ZonePosition::Natural,
                tapped: false,
                face_down: false,
                under_control_of: None,
            },
        );
        Self {
            engine: Engine::new(state),
            cards: Cards(vec![
                support::Cards::creature(10, false).0,
                original,
                copier,
                response,
            ]),
            creatures,
            original: original_id,
            copier: copier_id,
            response: response_id,
        }
    }
    fn next(&mut self) -> mtg_engine::Choice {
        mtg_verify::scenario::next_choice(&mut self.engine, &self.cards)
            .unwrap()
            .unwrap()
    }
    fn answer(&mut self, choice: &mtg_engine::Choice, answer: Answer) {
        self.engine
            .answer(&self.cards, choice.id, answer)
            .unwrap_or_else(|e| panic!("{:?}: {e:?}", choice.kind));
    }
    fn originals_on_stack(&mut self, mixed: bool, modal: bool) -> ObjectId {
        let first = self.next();
        self.answer(
            &first,
            Answer::Action(Action::Cast {
                object: self.original,
            }),
        );
        let mut group = 0;
        for _ in 0..20 {
            let choice = self.next();
            if matches!(choice.kind, ChoiceKind::Priority { .. }) {
                assert_eq!(choice.who, P0);
                let original = self.engine.state.objects_in(ZoneRef::shared(Zone::Stack))[0];
                self.answer(&choice, Answer::Pass);
                return original;
            }
            let answer = match choice.kind {
                ChoiceKind::ChooseX { .. } => Answer::Number(3),
                ChoiceKind::ChooseModes { .. } => Answer::Modes(vec![0]),
                ChoiceKind::ChooseTargets { .. } => {
                    let picked = if mixed && group == 0 {
                        vec![
                            Target::Object(self.creatures[0]),
                            Target::Object(self.creatures[1]),
                        ]
                    } else if mixed {
                        vec![]
                    } else {
                        vec![Target::Object(self.creatures[0])]
                    };
                    group += 1;
                    Answer::Targets(vec![picked])
                }
                _ => {
                    assert!(modal);
                    mtg_policy::well_formed(&choice, &self.engine.view_for(choice.who))
                }
            };
            self.answer(&choice, answer);
        }
        panic!("announcement did not finish");
    }
    fn start_copy(&mut self, original: ObjectId) {
        let priority = self.next();
        assert_eq!(priority.who, P1);
        self.answer(
            &priority,
            Answer::Action(Action::Cast {
                object: self.copier,
            }),
        );
        let targets = self.next();
        self.answer(
            &targets,
            Answer::Targets(vec![vec![Target::Object(original)]]),
        );
    }
    fn reach_copy(&mut self, retargets: Option<Vec<Vec<Target>>>) -> ObjectId {
        for _ in 0..40 {
            let choice = self.next();
            if let Some(id) = self
                .engine
                .state
                .objects
                .values()
                .find(|o| o.is_spell_copy)
                .map(|o| o.id)
            {
                return id;
            }
            let answer = match choice.kind {
                ChoiceKind::Priority { .. } => Answer::Pass,
                ChoiceKind::ChooseTargets { .. } => {
                    Answer::Targets(retargets.clone().expect("unexpected retargeting"))
                }
                _ => panic!("unexpected copy choice {:?}", choice.kind),
            };
            self.answer(&choice, answer);
        }
        panic!("copy was not created");
    }
    fn exile_before_copy_resolves(&mut self, object: ObjectId) {
        let priority = self.next();
        assert_eq!(priority.who, P1);
        self.answer(
            &priority,
            Answer::Action(Action::Cast {
                object: self.response,
            }),
        );
        let targets = self.next();
        self.answer(
            &targets,
            Answer::Targets(vec![vec![Target::Object(object)]]),
        );
    }
    fn finish(&mut self) {
        for _ in 0..40 {
            let choice = self.next();
            if self
                .engine
                .state
                .objects_in(ZoneRef::shared(Zone::Stack))
                .is_empty()
            {
                return;
            }
            self.answer(&choice, Answer::Pass);
        }
        panic!("stack failed to finish");
    }
}

#[test]
fn cr_707_10_copy_retains_x_but_does_not_copy_mana_spent() {
    let mut table = Table::new(false, true, false);
    let mana = &mut table.engine.state.players.get_mut(&P0).unwrap().mana;
    mana.amounts.fill(0);
    mana.amounts[mtg_core::Color::Green as usize] = 3;
    let AbilityKind::SpellEffect(Effect::Modal { modes, .. }) =
        &mut table.cards.0[1].abilities[0].kind
    else {
        panic!("fixture must have modal spell");
    };
    modes[0].1 = Effect::Sequence(vec![
        Effect::DealDamage {
            source: Selector::SelfSource,
            to: Selector::Target { index: 0 },
            amount: Value::X,
        },
        Effect::GainLife {
            who: Selector::You,
            amount: Value::ManaSpentOfColor(mtg_core::Color::Green),
        },
    ]);
    let original = table.originals_on_stack(false, true);
    assert_eq!(
        table.engine.state.objects[&original].mana_spent,
        vec![(Some(mtg_core::Color::Green), 3)]
    );
    table.start_copy(original);
    let copy = table.reach_copy(None);
    let copy_mana = table.engine.state.objects[&copy].mana_spent.clone();
    let context = table.engine.state.objects[&copy]
        .cast_context
        .as_ref()
        .unwrap();
    assert_eq!(context.x, 3);
    let context_mana = context.mana_spent.clone();
    table.finish();
    assert_eq!(table.engine.state.objects[&table.creatures[0]].damage, 6);
    assert_eq!(table.engine.state.players[&P0].life, 23);
    assert_eq!(
        table.engine.state.players[&P1].life, 20,
        "CR 707.10: mana is not an object; the copy gains no life from original payment"
    );
    assert!(copy_mana.is_empty());
    assert!(context_mana.is_empty());
}

#[test]
fn cr_707_10_copy_retains_x_mode_targets_but_is_owned_and_controlled_by_copying_player() {
    let mut table = Table::new(false, true, false);
    let original = table.originals_on_stack(false, true);
    table.start_copy(original);
    let copy = table.reach_copy(None);
    let object = &table.engine.state.objects[&copy];
    assert_eq!(object.controller, P1);
    assert_eq!(
        object.owner, P1,
        "a copied spell is owned by its controller, not original card owner"
    );
    let context = object.cast_context.as_ref().unwrap();
    assert_eq!(context.x, 3);
    assert_eq!(context.modes, vec![0]);
    assert_eq!(context.targets, vec![Target::Object(table.creatures[0])]);
    assert_eq!(
        table.engine.state.spells_cast_this_turn, 2,
        "creating the copy is not casting a third spell"
    );
    assert!(!table.engine.log.iter().any(|entry| matches!(entry.event,
        Event::SpellCast { object, .. } if object == copy)));
    table.finish();
    assert_eq!(table.engine.state.objects[&table.creatures[0]].damage, 6);
    assert_eq!(table.engine.state.players[&P0].life, 21);
    assert_eq!(table.engine.state.players[&P1].life, 21);
    assert!(
        !table.engine.state.objects.contains_key(&copy),
        "a resolved instant copy ceases to exist rather than becoming another card"
    );
    assert!(table.engine.log.iter().any(|entry| matches!(entry.event,
        Event::DamageMarked { source, amount: 3, .. } if source == copy)));
}

#[test]
fn cr_707_10c_legal_new_target_changes_copy_without_changing_original() {
    let mut table = Table::new(false, false, true);
    let original = table.originals_on_stack(false, false);
    table.start_copy(original);
    let copy = table.reach_copy(Some(vec![vec![Target::Object(table.creatures[2])]]));
    assert_eq!(
        table.engine.state.objects[&copy]
            .cast_context
            .as_ref()
            .unwrap()
            .targets,
        vec![Target::Object(table.creatures[2])]
    );
    assert_eq!(
        table.engine.state.objects[&original]
            .cast_context
            .as_ref()
            .unwrap()
            .targets,
        vec![Target::Object(table.creatures[0])]
    );
    table.finish();
    assert_eq!(table.engine.state.objects[&table.creatures[0]].damage, 2);
    assert_eq!(table.engine.state.objects[&table.creatures[2]].damage, 2);
}

#[test]
fn cr_707_10_copy_preserves_mixed_cardinality_and_unchosen_optional_slot() {
    let mut table = Table::new(true, false, false);
    let original = table.originals_on_stack(true, false);
    table.start_copy(original);
    let copy = table.reach_copy(None);
    let before = table.engine.state.objects[&original]
        .cast_context
        .as_ref()
        .unwrap();
    let after = table.engine.state.objects[&copy]
        .cast_context
        .as_ref()
        .unwrap();
    assert_eq!(after.targets, before.targets);
    assert_eq!(after.empty_slots, before.empty_slots);
    assert_eq!(after.target_specs, before.target_specs);
    assert_eq!(after.target_groups, before.target_groups);
    assert_eq!(after.empty_slots.len(), 1);
    assert_eq!(after.targets.len(), 3);
}

#[test]
fn cr_707_10c_retargeting_preserves_two_actual_targets_and_zero_optional_targets() {
    let mut table = Table::new(true, false, true);
    let original = table.originals_on_stack(true, false);
    table.start_copy(original);
    // Only two real targets were announced. The unchosen optional clause is
    // not a third target that permission to retarget could populate.
    let copy = table.reach_copy(Some(vec![
        vec![Target::Object(table.creatures[2])],
        vec![Target::Object(table.creatures[0])],
    ]));
    let context = table.engine.state.objects[&copy]
        .cast_context
        .as_ref()
        .unwrap();
    assert_eq!(context.empty_slots, vec![2]);
    assert_eq!(
        context.targets[..2],
        [
            Target::Object(table.creatures[2]),
            Target::Object(table.creatures[0])
        ]
    );
    assert_eq!(
        table.engine.state.objects[&original]
            .cast_context
            .as_ref()
            .unwrap()
            .targets[..2],
        [
            Target::Object(table.creatures[0]),
            Target::Object(table.creatures[1])
        ]
    );
}

#[test]
fn cr_707_10c_can_retain_an_illegal_target_and_608_2b_still_prevents_resolution() {
    let mut table = Table::new(false, false, true);
    let original = table.originals_on_stack(false, false);
    table.start_copy(original);
    let old_target = table.creatures[0];
    table.exile_before_copy_resolves(old_target);
    let copy = table.reach_copy(Some(vec![vec![Target::Object(old_target)]]));
    assert!(!table.engine.state.objects.contains_key(&old_target));
    assert_eq!(
        table.engine.state.objects[&copy]
            .cast_context
            .as_ref()
            .unwrap()
            .targets,
        vec![Target::Object(old_target)]
    );
    table.finish();
    assert_eq!(table.engine.state.players[&P0].life, 20);
    assert_eq!(table.engine.state.players[&P1].life, 20);
    assert!(
        table
            .engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
    );
}

#[test]
fn cr_707_10c_new_illegal_target_is_rejected_but_copy_choice_remains_pending() {
    let mut table = Table::new(false, false, true);
    let original = table.originals_on_stack(false, false);
    table.start_copy(original);
    for _ in 0..20 {
        let choice = table.next();
        if matches!(choice.kind, ChoiceKind::ChooseTargets { .. }) {
            assert!(
                table
                    .engine
                    .answer(
                        &table.cards,
                        choice.id,
                        Answer::Targets(vec![vec![Target::Player(P0)]])
                    )
                    .is_err(),
                "permission to keep an existing illegal target cannot allow a new illegal target"
            );
            let retry = table.next();
            assert_eq!(retry.id, choice.id);
            table.answer(
                &retry,
                Answer::Targets(vec![vec![Target::Object(table.creatures[2])]]),
            );
            table.reach_copy(None);
            return;
        }
        table.answer(&choice, Answer::Pass);
    }
    panic!("no copy target decision");
}

#[test]
fn cr_115_3_copy_retargeting_cannot_duplicate_target_within_one_two_target_instance() {
    let mut table = Table::new(true, false, true);
    let original = table.originals_on_stack(true, false);
    table.start_copy(original);
    for _ in 0..20 {
        let choice = table.next();
        if matches!(choice.kind, ChoiceKind::ChooseTargets { .. }) {
            let duplicate = Target::Object(table.creatures[2]);
            assert!(
                table
                    .engine
                    .answer(
                        &table.cards,
                        choice.id,
                        Answer::Targets(vec![vec![duplicate], vec![duplicate]])
                    )
                    .is_err(),
                "two members of one instance of target must remain distinct on the copy"
            );
            let retry = table.next();
            assert_eq!(retry.id, choice.id);
            table.answer(
                &retry,
                Answer::Targets(vec![
                    vec![duplicate],
                    vec![Target::Object(table.creatures[0])],
                ]),
            );
            table.reach_copy(None);
            return;
        }
        table.answer(&choice, Answer::Pass);
    }
    panic!("no mixed copy target decision");
}

#[test]
fn cr_115_3_copy_can_reuse_one_target_across_separate_target_instances() {
    let mut table = Table::new(false, false, true);
    table.cards.0[1].abilities[0]
        .targets
        .push(target(Zone::Battlefield, 1, false));
    if let AbilityKind::SpellEffect(Effect::Sequence(effects)) =
        &mut table.cards.0[1].abilities[0].kind
    {
        effects.push(Effect::Tap {
            what: Selector::Target { index: 1 },
        });
    } else {
        panic!("fixture must specify its second targeted action");
    }
    let original = table.originals_on_stack(false, false);
    table.start_copy(original);
    let reused = Target::Object(table.creatures[2]);
    let copy = table.reach_copy(Some(vec![vec![reused], vec![reused]]));
    assert_eq!(
        table.engine.state.objects[&copy]
            .cast_context
            .as_ref()
            .unwrap()
            .targets,
        vec![reused, reused]
    );
    table.finish();
    assert_eq!(table.engine.state.objects[&table.creatures[2]].damage, 2);
    assert!(table.engine.state.objects[&table.creatures[2]].tapped);
    assert_eq!(table.engine.state.objects[&table.creatures[0]].damage, 2);
    assert!(table.engine.state.objects[&table.creatures[0]].tapped);
}
