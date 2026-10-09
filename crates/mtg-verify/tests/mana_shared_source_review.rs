//! Independent payment enumeration grouped by physical permanent, not ability.
//! Scope: unrestricted tapped abilities making exactly one colored/colorless mana.
use mtg_core::{AbilityId, Color, ManaCost, ManaSymbol, ObjectId};
use mtg_engine::mana::{self, ManaSource};
use mtg_ir::ManaOutput;
use proptest::prelude::*;
use std::collections::BTreeSet;
const COLORS: [Color; 5] = [
    Color::White,
    Color::Blue,
    Color::Black,
    Color::Red,
    Color::Green,
];
fn choices(mask: u8) -> Vec<usize> {
    if mask == 0 {
        vec![5]
    } else {
        (0..5).filter(|i| mask & (1 << i) != 0).collect()
    }
}
fn sources(groups: &[Vec<u8>]) -> Vec<ManaSource> {
    groups
        .iter()
        .enumerate()
        .flat_map(|(object, abilities)| {
            abilities
                .iter()
                .enumerate()
                .map(move |(ability, mask)| ManaSource {
                    object: ObjectId(object as u32 + 1),
                    ability: AbilityId(ability as u16),
                    outputs: vec![if *mask == 0 {
                        ManaOutput::Colorless
                    } else if mask.count_ones() == 1 {
                        ManaOutput::Colored(COLORS[choices(*mask)[0]])
                    } else {
                        ManaOutput::AnyOf(choices(*mask).into_iter().map(|i| COLORS[i]).collect())
                    }],
                    taps: true,
                    sacrifices: false,
                    only: None,
                    spells_only: false,
                })
        })
        .collect()
}
fn oracle(groups: &[Vec<u8>], available: [u16; 6], required: [u16; 6], generic: u8) -> bool {
    // Independently enumerate no activation or one ability/output per object.
    if groups.is_empty() {
        return (0..6).all(|i| available[i] >= required[i])
            && (0..6)
                .map(|i| available[i].saturating_sub(required[i]) as u32)
                .sum::<u32>()
                >= generic as u32;
    }
    if oracle(&groups[1..], available, required, generic) {
        return true;
    }
    for ability in &groups[0] {
        for slot in choices(*ability) {
            let mut produced = available;
            produced[slot] += 1;
            if oracle(&groups[1..], produced, required, generic) {
                return true;
            }
        }
    }
    false
}
fn cost(required: [u16; 6], generic: u8) -> ManaCost {
    let mut symbols = vec![ManaSymbol::Generic(generic)];
    for slot in 0..6 {
        for _ in 0..required[slot] {
            symbols.push(if slot == 5 {
                ManaSymbol::Colorless
            } else {
                ManaSymbol::Colored(COLORS[slot])
            });
        }
    }
    ManaCost { symbols }
}
fn check(groups: Vec<Vec<u8>>, pool: [u16; 6], required: [u16; 6], generic: u8) {
    let sources = sources(&groups);
    let payment = mana::plan_with(
        &mana::requirements(&cost(required, generic), 0),
        &sources,
        pool,
        20,
    );
    assert_eq!(
        payment.is_some(),
        oracle(&groups, pool, required, generic),
        "groups={groups:?} pool={pool:?} required={required:?} generic={generic}"
    );
    if let Some(payment) = payment {
        let mut available = pool;
        let mut used = BTreeSet::new();
        assert_eq!(payment.life, 0);
        for (object, ability, selected) in payment.activate {
            assert!(used.insert(object), "same tapped permanent activated twice");
            let source = sources
                .iter()
                .find(|s| s.object == object && s.ability == ability)
                .expect("recorded ability identity exists");
            assert_eq!(source.outputs.len(), 1);
            let slot = selected.map_or(5, |c| c as usize);
            assert!(
                choices(groups[object.0 as usize - 1][ability.0 as usize]).contains(&slot),
                "ability cannot produce selected output"
            );
            available[slot] += 1;
        }
        let mut generic_paid = 0u32;
        for slot in 0..6 {
            assert!(
                payment.spend[slot] <= available[slot],
                "cannot spend unavailable mana"
            );
            assert!(
                payment.spend[slot] >= required[slot],
                "must pay exact colored/colorless symbols"
            );
            generic_paid += u32::from(payment.spend[slot] - required[slot]);
        }
        assert_eq!(generic_paid, generic as u32);
    }
}
#[test]
fn one_permanent_with_two_tap_abilities_cannot_pay_generic_two() {
    check(vec![vec![1, 2]], [0; 6], [0; 6], 2);
}
#[test]
fn chosen_color_uses_matching_ability_and_separate_objects_pay_two() {
    check(vec![vec![1, 2]], [0; 6], [0, 1, 0, 0, 0, 0], 0);
    check(vec![vec![1, 2], vec![1, 2]], [0; 6], [1, 1, 0, 0, 0, 0], 0);
}
#[test]
fn same_permanent_can_make_green_or_colorless_but_cannot_make_both() {
    check(vec![vec![16, 0]], [0; 6], [0, 0, 0, 0, 1, 0], 0);
    check(vec![vec![16, 0]], [0; 6], [0, 0, 0, 0, 0, 1], 0);
    check(vec![vec![16, 0]], [0; 6], [0, 0, 0, 0, 1, 1], 0);
}
proptest! {
    #[test]
    fn grouped_activation_enumeration_agrees_with_production_payment(
        groups in prop::collection::vec(prop::collection::vec(0u8..=31, 1..=3), 0..=4),
        pool in prop::array::uniform6(0u16..=1),
        required in prop::array::uniform6(0u16..=1),
        generic in 0u8..=3,
    ) { check(groups, pool, required, generic); }
}

#[test]
fn engine_pays_colorless_with_second_tap_ability_and_resolves_life_gain() {
    use mtg_core::{CardId, CardType, PlayerId, Zone, ZoneRef};
    use mtg_engine::{Action, Answer, ChoiceKind};
    use mtg_ir::{
        Ability, AbilityKind, AdditionalCost, Cost, Effect, Selector, Value,
        ability::ActivationTiming,
    };
    use mtg_verify::scenario::{
        GameScenario, Metadata, ScenarioAction, ScenarioAssertions, ScenarioObject, ZoneAssertion,
        next_choice, run,
    };
    use std::{collections::BTreeMap, path::PathBuf};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut scenario = GameScenario::load(&root.join("tests/replays/basic_casting.json")).unwrap();
    let mut land = scenario.cards[0].clone();
    land.name = "Independent G or C tapping permanent".into();
    land.card_types = vec![CardType::Land];
    land.power = None;
    land.toughness = None;
    land.abilities = [ManaOutput::Colored(Color::Green), ManaOutput::Colorless]
        .into_iter()
        .enumerate()
        .map(|(index, output)| Ability {
            id: AbilityId(index as u16),
            targets: vec![],
            source_text: None,
            kind: AbilityKind::Activated {
                cost: Cost {
                    additional: vec![AdditionalCost::Tap {
                        what: Selector::SelfSource,
                    }],
                    ..Cost::free()
                },
                effect: Effect::AddMana {
                    who: Selector::You,
                    produces: vec![output],
                },
                functions_from: Zone::Battlefield,
                is_mana_ability: true,
                is_loyalty_ability: false,
                timing: ActivationTiming::Instant,
            },
        })
        .collect();
    let mut spell = scenario.cards[0].clone();
    spell.name = "Independent colorless-cost life gain".into();
    spell.card_types = vec![CardType::Instant];
    spell.power = None;
    spell.toughness = None;
    spell.mana_cost.symbols = vec![ManaSymbol::Colorless];
    spell.abilities = vec![Ability {
        id: AbilityId(0),
        targets: vec![],
        source_text: None,
        kind: AbilityKind::SpellEffect(Effect::GainLife {
            who: Selector::You,
            amount: Value::Fixed(3),
        }),
    }];
    scenario.cards = vec![land, spell];
    scenario.card_db_hash = scenario.card_hash();
    scenario.initial_state.objects = vec![
        ScenarioObject {
            card: CardId(0),
            owner: PlayerId(0),
            controller: None,
            zone: ZoneRef::shared(Zone::Battlefield),
            tapped: false,
            phased_out: false,
            damage: 0,
            counters: BTreeMap::new(),
        },
        ScenarioObject {
            card: CardId(1),
            owner: PlayerId(0),
            controller: None,
            zone: ZoneRef::of(Zone::Hand, PlayerId(0)),
            tapped: false,
            phased_out: false,
            damage: 0,
            counters: BTreeMap::new(),
        },
    ];
    scenario.metadata = Metadata {
        issue: Some("local_mana_colorless_alternative".into()),
        description: "Confirmed shared-source planner defect: combining a permanent's G and C tapping abilities lost its colorless alternative. A C-cost instant must remain castable, activate only the C ability once, tap the land, spend available mana, and resolve for three life.".into(),
        rules: vec!["106.1b".into(), "107.4c".into(), "601.2h".into(), "605.1a".into()],
        first_affected: None, fixed_in: Some("working-tree explicit colorless unit alternative".into()),
    };
    scenario.actions = [
        (
            PlayerId(0),
            Answer::Action(Action::Cast {
                object: ObjectId(2),
            }),
        ),
        (PlayerId(0), Answer::Pass),
        (PlayerId(1), Answer::Pass),
    ]
    .into_iter()
    .map(|(who, answer)| ScenarioAction {
        who,
        answer,
        expected_rejection: false,
        expected_choice: None,
        expected_state: None,
        expected_digest: None,
    })
    .collect();
    scenario.expected = ScenarioAssertions::default();
    // Expected Magic behavior is specified before recording any checkpoints.
    scenario.expected.life.insert(PlayerId(0), 23);
    for (zone, card) in [
        (ZoneRef::shared(Zone::Battlefield), CardId(0)),
        (ZoneRef::of(Zone::Graveyard, PlayerId(0)), CardId(1)),
    ] {
        scenario.expected.zones.push(ZoneAssertion {
            zone,
            card: Some(card),
            owner: Some(PlayerId(0)),
            count: 1,
            counters: BTreeMap::new(),
        });
    }
    let (mut engine, cards) = scenario.setup().unwrap();
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    let ChoiceKind::Priority { legal } = &choice.kind else {
        panic!("priority")
    };
    assert!(legal.actions.contains(&Action::Cast {
        object: ObjectId(2)
    }));
    for (index, action) in scenario.actions.iter().enumerate() {
        let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
        assert_eq!(choice.who, action.who);
        engine
            .answer(&cards, choice.id, action.answer.clone())
            .unwrap();
        let _ = next_choice(&mut engine, &cards).unwrap();
        if index == 0 {
            assert!(engine.state.objects[&ObjectId(1)].tapped);
            assert_eq!(engine.state.player(PlayerId(0)).mana.amounts, [0; 6]);
            let produced: Vec<_> = engine
                .log
                .iter()
                .filter_map(|event| match event.event {
                    mtg_core::Event::ManaAdded {
                        player,
                        color,
                        amount,
                    } => Some((player, color, amount)),
                    _ => None,
                })
                .collect();
            let spent: Vec<_> = engine
                .log
                .iter()
                .filter_map(|event| match event.event {
                    mtg_core::Event::ManaSpent {
                        player,
                        color,
                        amount,
                    } => Some((player, color, amount)),
                    _ => None,
                })
                .collect();
            assert_eq!(produced, vec![(PlayerId(0), None, 1)]);
            assert_eq!(spent, vec![(PlayerId(0), None, 1)]);
        }
    }
    assert_eq!(engine.state.player(PlayerId(0)).life, 23);
    assert!(engine.state.objects[&ObjectId(1)].tapped);
    let (report, artifact) = run(&scenario, true).unwrap();
    assert!(report.pass, "{}", report.message);
    if let Ok(destination) = std::env::var("MTGO_RECORD_MANA_FIXTURE") {
        use std::io::Write;
        let mut file = std::fs::File::create_new(destination).unwrap();
        file.write_all(&serde_json::to_vec_pretty(&artifact).unwrap())
            .unwrap();
    }
}
