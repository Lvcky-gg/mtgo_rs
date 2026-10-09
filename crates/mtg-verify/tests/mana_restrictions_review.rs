//! Independent spell-restricted mana lifecycle propositions.
use mtg_core::{CardId, CardType, ManaSymbol, ObjectId};
use mtg_engine::{Action, Answer, ChoiceKind};
use mtg_ir::{AbilityKind, Effect, ManaOutput, ObjectFilter, Value};
use mtg_verify::scenario::{GameScenario, next_choice};
fn scenario(amount: i32, spell_type: CardType) -> GameScenario {
    let mut s = GameScenario::load(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/regressions/issue_local_mana_colorless_alternative.json"),
    )
    .unwrap();
    s.cards[0].abilities.truncate(1);
    let AbilityKind::Activated { effect, .. } = &mut s.cards[0].abilities[0].kind else {
        panic!("mana ability")
    };
    *effect = Effect::SpendOnly {
        only: ObjectFilter::HasType(CardType::Creature),
        spells_only: false,
        effect: Box::new(Effect::AddMana {
            who: mtg_ir::Selector::You,
            produces: vec![ManaOutput::Repeated {
                amount: Value::Fixed(amount),
                output: Box::new(ManaOutput::Colorless),
            }],
        }),
    };
    s.cards[1].mana_cost.symbols = vec![ManaSymbol::Generic(1)];
    s.cards[1].card_types = vec![spell_type];
    if spell_type == CardType::Creature {
        s.cards[1].power = Some(1);
        s.cards[1].toughness = Some(1);
    }
    s.card_db_hash = s.card_hash();
    s
}
fn offered(s: &GameScenario) -> bool {
    let (mut engine, cards) = s.setup().unwrap();
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    let ChoiceKind::Priority { legal } = choice.kind else {
        panic!("priority")
    };
    legal.actions.contains(&Action::Cast {
        object: ObjectId(2),
    })
}
#[test]
fn creature_only_source_can_pay_creature_but_not_instant() {
    assert!(offered(&scenario(1, CardType::Creature)));
    assert!(!offered(&scenario(1, CardType::Instant)));
}
#[test]
fn automatic_leftover_remains_restricted_after_eligible_spell_payment() {
    let mut s = scenario(2, CardType::Creature);
    let mut instant = s.cards[1].clone();
    instant.card_types = vec![CardType::Instant];
    instant.power = None;
    instant.toughness = None;
    s.cards.push(instant);
    let mut object = s.initial_state.objects[1].clone();
    object.card = CardId(2);
    s.initial_state.objects.push(object);
    s.card_db_hash = s.card_hash();
    let (mut engine, cards) = s.setup().unwrap();
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    engine
        .answer(
            &cards,
            choice.id,
            Answer::Action(Action::Cast {
                object: ObjectId(2),
            }),
        )
        .unwrap();
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    assert!(engine.state.objects[&ObjectId(1)].tapped);
    assert_eq!(engine.state.player(mtg_core::PlayerId(0)).mana.total(), 1);
    let ChoiceKind::Priority { legal } = choice.kind else {
        panic!("priority")
    };
    assert!(
        !legal.actions.contains(&Action::Cast {
            object: ObjectId(3)
        }),
        "the remaining creature-only mana must not cast an instant"
    );
}

#[test]
fn unsupported_manual_restricted_activation_rejects_without_state_change() {
    let s = scenario(2, CardType::Creature);
    let (mut engine, cards) = s.setup().unwrap();
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    let before = mtg_verify::canonical::CanonicalGameState::from_engine(&engine);
    let result = engine.answer(
        &cards,
        choice.id,
        Answer::Action(Action::ActivateManaAbility {
            source: ObjectId(1),
            ability: mtg_core::AbilityId(0),
            color: None,
        }),
    );
    assert!(result.is_err());
    assert_eq!(
        mtg_verify::canonical::CanonicalGameState::from_engine(&engine),
        before
    );
    assert_eq!(
        next_choice(&mut engine, &cards).unwrap().unwrap().id,
        choice.id
    );
}

#[test]
fn powerstone_restriction_allows_abilities_but_forbids_nonartifact_spells() {
    let mut s = scenario(1, CardType::Instant);
    let AbilityKind::Activated {
        effect: Effect::SpendOnly {
            only, spells_only, ..
        },
        ..
    } = &mut s.cards[0].abilities[0].kind
    else {
        panic!("restricted source")
    };
    *only = ObjectFilter::HasType(CardType::Artifact);
    *spells_only = true;
    let mut ability = s.cards[0].abilities[0].clone();
    ability.id = mtg_core::AbilityId(1);
    let AbilityKind::Activated {
        cost,
        effect,
        is_mana_ability,
        ..
    } = &mut ability.kind
    else {
        panic!("ability")
    };
    cost.additional.clear();
    cost.mana.symbols = vec![ManaSymbol::Generic(1)];
    *effect = Effect::GainLife {
        who: mtg_ir::Selector::You,
        amount: Value::Fixed(1),
    };
    *is_mana_ability = false;
    s.cards[0].abilities.push(ability);
    s.card_db_hash = s.card_hash();
    let (mut engine, cards) = s.setup().unwrap();
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    let ChoiceKind::Priority { legal } = choice.kind else {
        panic!("priority")
    };
    assert!(!legal.actions.contains(&Action::Cast {
        object: ObjectId(2)
    }));
    assert!(legal.actions.contains(&Action::ActivateAbility {
        source: ObjectId(1),
        ability: mtg_core::AbilityId(1)
    }));
    s.cards[1].card_types.push(CardType::Artifact);
    s.card_db_hash = s.card_hash();
    assert!(offered(&s));
}

fn add_spell(s: &mut GameScenario, card_type: CardType, cost: u8) -> ObjectId {
    let mut card = s.cards[1].clone();
    card.card_types = vec![card_type];
    card.mana_cost.symbols = vec![ManaSymbol::Generic(cost)];
    if card_type != CardType::Creature {
        card.power = None;
        card.toughness = None;
    }
    let card_id = CardId(s.cards.len() as u32);
    s.cards.push(card);
    let mut object = s.initial_state.objects[1].clone();
    object.card = card_id;
    s.initial_state.objects.push(object);
    s.card_db_hash = s.card_hash();
    ObjectId(s.initial_state.objects.len() as u32)
}
fn cast(
    engine: &mut mtg_engine::Engine,
    cards: &mtg_verify::scenario::ScenarioCards,
    object: ObjectId,
) {
    let choice = next_choice(engine, cards).unwrap().unwrap();
    engine
        .answer(cards, choice.id, Answer::Action(Action::Cast { object }))
        .unwrap();
    next_choice(engine, cards).unwrap();
}
fn legal(
    engine: &mut mtg_engine::Engine,
    cards: &mtg_verify::scenario::ScenarioCards,
) -> Vec<Action> {
    let choice = next_choice(engine, cards).unwrap().unwrap();
    let ChoiceKind::Priority { legal } = choice.kind else {
        panic!("priority")
    };
    legal.actions
}
fn paid_ability(s: &mut GameScenario) -> Action {
    let mut ability = s.cards[0].abilities[0].clone();
    ability.id = mtg_core::AbilityId(1);
    let AbilityKind::Activated {
        cost,
        effect,
        is_mana_ability,
        ..
    } = &mut ability.kind
    else {
        panic!("ability")
    };
    cost.additional.clear();
    cost.mana.symbols = vec![ManaSymbol::Generic(1)];
    *effect = Effect::GainLife {
        who: mtg_ir::Selector::You,
        amount: Value::Fixed(1),
    };
    *is_mana_ability = false;
    s.cards[0].abilities.push(ability);
    s.card_db_hash = s.card_hash();
    Action::ActivateAbility {
        source: ObjectId(1),
        ability: mtg_core::AbilityId(1),
    }
}
#[test]
fn mixed_free_and_restricted_pool_spends_only_free_mana_for_instant() {
    let mut s = scenario(2, CardType::Creature);
    let first = add_spell(&mut s, CardType::Instant, 1);
    let second = add_spell(&mut s, CardType::Instant, 1);
    let free_source = add_free_source(&mut s);
    let (mut engine, cards) = s.setup().unwrap();
    cast(&mut engine, &cards, ObjectId(2));
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    engine
        .answer(
            &cards,
            choice.id,
            Answer::Action(Action::ActivateManaAbility {
                source: free_source,
                ability: mtg_core::AbilityId(0),
                color: None,
            }),
        )
        .unwrap();
    assert!(legal(&mut engine, &cards).contains(&Action::Cast { object: first }));
    cast(&mut engine, &cards, first);
    assert_eq!(engine.state.player(mtg_core::PlayerId(0)).mana.total(), 1);
    assert!(!legal(&mut engine, &cards).contains(&Action::Cast { object: second }));
}
#[test]
fn eligible_creature_can_combine_free_and_restricted_mana() {
    let mut s = scenario(2, CardType::Creature);
    let second = add_spell(&mut s, CardType::Creature, 2);
    let free_source = add_free_source(&mut s);
    let (mut engine, cards) = s.setup().unwrap();
    cast(&mut engine, &cards, ObjectId(2));
    settle(&mut engine, &cards);
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    engine
        .answer(
            &cards,
            choice.id,
            Answer::Action(Action::ActivateManaAbility {
                source: free_source,
                ability: mtg_core::AbilityId(0),
                color: None,
            }),
        )
        .unwrap();
    assert!(legal(&mut engine, &cards).contains(&Action::Cast { object: second }));
    cast(&mut engine, &cards, second);
    assert_eq!(engine.state.player(mtg_core::PlayerId(0)).mana.total(), 0);
}
#[test]
fn creature_only_leftover_cannot_pay_nonmatching_activated_ability() {
    let mut s = scenario(2, CardType::Creature);
    let action = paid_ability(&mut s);
    let (mut engine, cards) = s.setup().unwrap();
    cast(&mut engine, &cards, ObjectId(2));
    assert!(!legal(&mut engine, &cards).contains(&action));
}
#[test]
fn powerstone_leftover_allows_ability_and_artifact_but_not_other_instant() {
    let mut s = scenario(2, CardType::Artifact);
    let AbilityKind::Activated {
        effect: Effect::SpendOnly {
            only, spells_only, ..
        },
        ..
    } = &mut s.cards[0].abilities[0].kind
    else {
        panic!("source")
    };
    *only = ObjectFilter::HasType(CardType::Artifact);
    *spells_only = true;
    let instant = add_spell(&mut s, CardType::Instant, 1);
    let artifact = add_spell(&mut s, CardType::Artifact, 1);
    let ability = paid_ability(&mut s);
    let (mut engine, cards) = s.setup().unwrap();
    cast(&mut engine, &cards, ObjectId(2));
    settle(&mut engine, &cards);
    let legal = legal(&mut engine, &cards);
    assert!(legal.contains(&ability));
    assert!(legal.contains(&Action::Cast { object: artifact }));
    assert!(!legal.contains(&Action::Cast { object: instant }));
}

proptest::proptest! {
    #[test]
    fn independent_typed_floating_restriction_capacity(
        context in 0u8..4, artifact_only in proptest::bool::ANY,
        spells_only in proptest::bool::ANY, restricted in 0u16..=3,
        free in 0u16..=3, cost in 0u8..=6,
    ) {
        use mtg_core::PlayerId;
        let card_type = match context {0 => CardType::Creature, 1 => CardType::Artifact, _ => CardType::Instant};
        let mut s = scenario(1, card_type);
        s.cards[1].mana_cost.symbols = vec![ManaSymbol::Generic(cost)];
        let action = if context == 3 {
            let action = paid_ability(&mut s);
            let AbilityKind::Activated {cost: ability_cost, ..} = &mut s.cards[0].abilities[1].kind else {panic!("ability")};
            ability_cost.mana.symbols = vec![ManaSymbol::Generic(cost)]; action
        } else {Action::Cast {object: ObjectId(2)}};
        s.card_db_hash = s.card_hash();
        let (mut engine, cards) = s.setup().unwrap();
        engine.state.objects.get_mut(&ObjectId(1)).unwrap().tapped = true;
        engine.state.players.get_mut(&mtg_core::PlayerId(0)).unwrap().mana.amounts = [0,0,0,0,0,free + restricted];
        if restricted > 0 {
            engine.state.restricted_mana.push(mtg_engine::state::RestrictedMana {
                player: PlayerId(0), source: ObjectId(1), amounts: [0,0,0,0,0,restricted],
                restrictions: vec![(ObjectFilter::HasType(if artifact_only {CardType::Artifact} else {CardType::Creature}), spells_only)],
            });
        }
        // Creature-only applies to every cost's object; Powerstone's restriction only applies to spells.
        // The ability's source is a land, so neither typed predicate matches it.
        let eligible = (context == 3 && spells_only) || (context == 0 && !artifact_only) || (context == 1 && artifact_only);
        let capacity = free + if eligible {restricted} else {0};
        proptest::prop_assert_eq!(legal(&mut engine, &cards).contains(&action), u16::from(cost) <= capacity,
            "context={} artifact_only={} spells_only={} restricted={} free={} cost={}", context, artifact_only, spells_only, restricted, free, cost);
    }
}

#[test]
fn restriction_changes_digest_and_other_player_does_not_supply_capacity() {
    use mtg_core::PlayerId;
    let s = scenario(1, CardType::Instant);
    let (mut engine, cards) = s.setup().unwrap();
    engine.state.objects.get_mut(&ObjectId(1)).unwrap().tapped = true;
    engine
        .state
        .players
        .get_mut(&mtg_core::PlayerId(0))
        .unwrap()
        .mana
        .amounts = [0, 0, 0, 0, 0, 1];
    engine
        .state
        .restricted_mana
        .push(mtg_engine::state::RestrictedMana {
            player: PlayerId(0),
            source: ObjectId(1),
            amounts: [0, 0, 0, 0, 0, 1],
            restrictions: vec![(ObjectFilter::HasType(CardType::Creature), false)],
        });
    let first = mtg_verify::canonical::CanonicalGameState::from_engine(&engine)
        .unwrap()
        .digest();
    engine.state.restricted_mana[0].restrictions[0].0 = ObjectFilter::HasType(CardType::Instant);
    assert_ne!(
        mtg_verify::canonical::CanonicalGameState::from_engine(&engine)
            .unwrap()
            .digest(),
        first
    );
    assert!(legal(&mut engine, &cards).contains(&Action::Cast {
        object: ObjectId(2)
    }));
    let (mut engine, cards) = s.setup().unwrap();
    engine.state.objects.get_mut(&ObjectId(1)).unwrap().tapped = true;
    engine
        .state
        .restricted_mana
        .push(mtg_engine::state::RestrictedMana {
            player: PlayerId(1),
            source: ObjectId(1),
            amounts: [0, 0, 0, 0, 0, 1],
            restrictions: vec![(ObjectFilter::HasType(CardType::Instant), false)],
        });
    engine
        .state
        .players
        .get_mut(&mtg_core::PlayerId(1))
        .unwrap()
        .mana
        .amounts = [0, 0, 0, 0, 0, 1];
    engine.state.restricted_mana[0].player = PlayerId(1);
    assert!(!legal(&mut engine, &cards).contains(&Action::Cast {
        object: ObjectId(2)
    }));
}
fn settle(engine: &mut mtg_engine::Engine, cards: &mtg_verify::scenario::ScenarioCards) {
    for _ in 0..2 {
        let choice = next_choice(engine, cards).unwrap().unwrap();
        engine.answer(cards, choice.id, Answer::Pass).unwrap();
        next_choice(engine, cards).unwrap();
    }
}
fn add_free_source(s: &mut GameScenario) -> ObjectId {
    let mut card = s.cards[0].clone();
    let AbilityKind::Activated { effect, .. } = &mut card.abilities[0].kind else {
        panic!("source")
    };
    *effect = Effect::AddMana {
        who: mtg_ir::Selector::You,
        produces: vec![ManaOutput::Colorless],
    };
    let id = CardId(s.cards.len() as u32);
    s.cards.push(card);
    let mut object = s.initial_state.objects[0].clone();
    object.card = id;
    s.initial_state.objects.push(object);
    s.card_db_hash = s.card_hash();
    ObjectId(s.initial_state.objects.len() as u32)
}

#[test]
fn nested_restrictions_require_both_types() {
    for both in [false, true] {
        let mut s = scenario(1, CardType::Creature);
        if both {
            s.cards[1].card_types.push(CardType::Artifact);
        }
        s.card_db_hash = s.card_hash();
        let (mut engine, cards) = s.setup().unwrap();
        engine.state.objects.get_mut(&ObjectId(1)).unwrap().tapped = true;
        engine
            .state
            .players
            .get_mut(&mtg_core::PlayerId(0))
            .unwrap()
            .mana
            .add(None, 1);
        engine
            .state
            .restricted_mana
            .push(mtg_engine::state::RestrictedMana {
                player: mtg_core::PlayerId(0),
                source: ObjectId(1),
                amounts: [0, 0, 0, 0, 0, 1],
                restrictions: vec![
                    (ObjectFilter::HasType(CardType::Creature), false),
                    (ObjectFilter::HasType(CardType::Artifact), false),
                ],
            });
        assert_eq!(
            legal(&mut engine, &cards).contains(&Action::Cast {
                object: ObjectId(2)
            }),
            both
        );
    }
}
#[test]
fn invalid_provenance_cannot_pass_invariants() {
    let s = scenario(1, CardType::Instant);
    for kind in 0..4 {
        let (mut engine, cards) = s.setup().unwrap();
        engine
            .state
            .players
            .get_mut(&mtg_core::PlayerId(0))
            .unwrap()
            .mana
            .add(None, 1);
        let mut record = mtg_engine::state::RestrictedMana {
            player: mtg_core::PlayerId(0),
            source: ObjectId(1),
            amounts: [0, 0, 0, 0, 0, 1],
            restrictions: vec![(ObjectFilter::HasType(CardType::Creature), false)],
        };
        engine.state.restricted_mana.push(record.clone());
        mtg_verify::invariants::verify(&engine.state, &cards).unwrap();
        engine.state.restricted_mana.clear();
        match kind {
            0 => record.player = mtg_core::PlayerId(99),
            1 => record.restrictions.clear(),
            2 => record.amounts = [0; 6],
            _ => record.amounts[5] = 2,
        }
        engine.state.restricted_mana.push(record);
        assert!(
            mtg_verify::invariants::verify(&engine.state, &cards).is_err(),
            "invalid kind {kind}"
        );
    }
}
#[test]
fn phase_end_clears_restrictions_and_later_free_mana_remains_usable() {
    let mut s = scenario(2, CardType::Creature);
    let instant = add_spell(&mut s, CardType::Instant, 1);
    let free_source = add_free_source(&mut s);
    let (mut engine, cards) = s.setup().unwrap();
    cast(&mut engine, &cards, ObjectId(2));
    let initial_step = engine.state.step;
    for _ in 0..20 {
        let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
        engine.answer(&cards, choice.id, Answer::Pass).unwrap();
        next_choice(&mut engine, &cards).unwrap();
        if engine.state.step != initial_step {
            break;
        }
    }
    assert_ne!(engine.state.step, initial_step);
    assert_eq!(engine.state.player(mtg_core::PlayerId(0)).mana.total(), 0);
    assert!(engine.state.restricted_mana.is_empty());
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    engine
        .answer(
            &cards,
            choice.id,
            Answer::Action(Action::ActivateManaAbility {
                source: free_source,
                ability: mtg_core::AbilityId(0),
                color: None,
            }),
        )
        .unwrap();
    assert!(legal(&mut engine, &cards).contains(&Action::Cast { object: instant }));
}
#[test]
fn confirmed_leftover_bug_has_permanent_independently_asserted_fixture() {
    use mtg_core::{PlayerId, Zone, ZoneRef};
    use mtg_verify::scenario::{ScenarioAction, ScenarioAssertions, ZoneAssertion, run};
    use std::collections::BTreeMap;
    let mut s = scenario(2, CardType::Creature);
    let instant = add_spell(&mut s, CardType::Instant, 1);
    s.metadata.issue = Some("local_restricted_mana_leftover_leak".into());
    s.metadata.description="Creature-only C2 automatically pays generic1 creature; leftover C1 must remain restricted and the instant must not be offered. Neither spell has resolved and life remains twenty.".into();
    s.metadata.rules = vec!["106.6".into(), "601.2h".into()];
    s.metadata.fixed_in = Some("working-tree restricted floating mana provenance".into());
    let (mut engine, cards) = s.setup().unwrap();
    cast(&mut engine, &cards, ObjectId(2));
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    let ChoiceKind::Priority { legal } = &choice.kind else {
        panic!("priority")
    };
    assert!(!legal.actions.contains(&Action::Cast { object: instant }));
    assert_eq!(engine.state.player(PlayerId(0)).life, 20);
    assert_eq!(engine.state.player(PlayerId(0)).mana.total(), 1);
    s.actions = vec![
        ScenarioAction {
            who: PlayerId(0),
            answer: Answer::Action(Action::Cast {
                object: ObjectId(2),
            }),
            expected_rejection: false,
            expected_choice: None,
            expected_state: None,
            expected_digest: None,
        },
        ScenarioAction {
            who: PlayerId(0),
            answer: Answer::Pass,
            expected_rejection: false,
            expected_choice: Some(choice.kind),
            expected_state: None,
            expected_digest: None,
        },
    ];
    s.expected = ScenarioAssertions::default();
    s.expected.life.insert(PlayerId(0), 20);
    for (zone, card) in [
        (ZoneRef::shared(Zone::Battlefield), CardId(0)),
        (ZoneRef::shared(Zone::Stack), CardId(1)),
        (ZoneRef::of(Zone::Hand, PlayerId(0)), CardId(2)),
    ] {
        s.expected.zones.push(ZoneAssertion {
            zone,
            card: Some(card),
            owner: Some(PlayerId(0)),
            count: 1,
            counters: BTreeMap::new(),
        });
    }
    let (report, artifact) = run(&s, true).unwrap();
    assert!(report.pass, "{}", report.message);
    if std::env::var_os("MTGO_RECORD_RESTRICTED_FIXTURE").is_some() {
        use std::io::Write;
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/regressions/issue_local_restricted_mana_leftover_leak.json");
        std::fs::File::create_new(path)
            .unwrap()
            .write_all(&serde_json::to_vec_pretty(&artifact).unwrap())
            .unwrap();
    }
}
