//! Independent enumeration of mana-symbol alternatives and actual payment assignments.
//! Scope: ordinary, hybrid, monohybrid and phyrexian symbols; one-mana tap sources.
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
#[derive(Clone, Debug)]
struct Assignment {
    exact: [u16; 6],
    generic: u16,
    life: u32,
}
fn assignments(symbols: &[ManaSymbol], current: Assignment, results: &mut Vec<Assignment>) {
    let Some((symbol, rest)) = symbols.split_first() else {
        results.push(current);
        return;
    };
    let mut mana = |slot: usize| {
        let mut next = current.clone();
        next.exact[slot] += 1;
        assignments(rest, next, results);
    };
    match symbol {
        ManaSymbol::Colored(c) => mana(*c as usize),
        ManaSymbol::Colorless => mana(5),
        ManaSymbol::Generic(n) => {
            let mut next = current;
            next.generic += u16::from(*n);
            assignments(rest, next, results);
        }
        ManaSymbol::Hybrid(a, b) => {
            mana(*a as usize);
            mana(*b as usize);
        }
        ManaSymbol::MonoHybrid(n, c) => {
            mana(*c as usize);
            let mut next = current;
            next.generic += u16::from(*n);
            assignments(rest, next, results);
        }
        ManaSymbol::Phyrexian(c) => {
            mana(*c as usize);
            let mut next = current;
            next.life += 2;
            assignments(rest, next, results);
        }
        _ => panic!("unmodeled symbol"),
    }
}
fn variants(symbols: &[ManaSymbol]) -> Vec<Assignment> {
    let mut out = vec![];
    assignments(
        symbols,
        Assignment {
            exact: [0; 6],
            generic: 0,
            life: 0,
        },
        &mut out,
    );
    out
}
fn choices(mask: u8) -> Vec<usize> {
    if mask == 0 {
        vec![5]
    } else {
        (0..5).filter(|i| mask & (1 << i) != 0).collect()
    }
}
fn capacity(masks: &[u8], available: [u16; 6], assignment: &Assignment) -> bool {
    if masks.is_empty() {
        return (0..6).all(|i| available[i] >= assignment.exact[i])
            && (0..6)
                .map(|i| available[i].saturating_sub(assignment.exact[i]) as u32)
                .sum::<u32>()
                >= u32::from(assignment.generic);
    }
    if capacity(&masks[1..], available, assignment) {
        return true;
    }
    for slot in choices(masks[0]) {
        let mut next = available;
        next[slot] += 1;
        if capacity(&masks[1..], next, assignment) {
            return true;
        }
    }
    false
}
fn check(symbols: Vec<ManaSymbol>, pool: [u16; 6], masks: Vec<u8>, life: i32) {
    let alternatives = variants(&symbols);
    let possible = alternatives
        .iter()
        .any(|a| a.life <= life as u32 && capacity(&masks, pool, a));
    let sources: Vec<_> = masks
        .iter()
        .enumerate()
        .map(|(i, mask)| ManaSource {
            object: ObjectId(i as u32 + 1),
            ability: AbilityId(0),
            outputs: vec![if *mask == 0 {
                ManaOutput::Colorless
            } else {
                ManaOutput::AnyOf(choices(*mask).into_iter().map(|i| COLORS[i]).collect())
            }],
            taps: true,
            sacrifices: false,
            only: None,
            spells_only: false,
        })
        .collect();
    let payment = mana::plan_with(
        &mana::requirements(
            &ManaCost {
                symbols: symbols.clone(),
            },
            0,
        ),
        &sources,
        pool,
        life,
    );
    assert_eq!(
        payment.is_some(),
        possible,
        "symbols={symbols:?} pool={pool:?} sources={masks:?} life={life}"
    );
    if let Some(payment) = payment {
        assert!(payment.life <= life as u32);
        let mut available = pool;
        let mut used = BTreeSet::new();
        for (object, ability, color) in payment.activate {
            assert_eq!(ability, AbilityId(0));
            assert!(used.insert(object));
            let slot = color.map_or(5, |c| c as usize);
            assert!(choices(masks[object.0 as usize - 1]).contains(&slot));
            available[slot] += 1;
        }
        for (slot, amount) in available.iter().enumerate() {
            assert!(
                payment.spend[slot] <= *amount,
                "cannot spend unavailable mana"
            );
        }
        assert!(
            alternatives.iter().any(|a| a.life == payment.life
                && (0..6).all(|i| payment.spend[i] >= a.exact[i])
                && (0..6)
                    .map(|i| payment.spend[i].saturating_sub(a.exact[i]) as u32)
                    .sum::<u32>()
                    == u32::from(a.generic)),
            "actual spend/life must match one complete symbol assignment"
        );
    }
}
#[test]
fn generic_plus_phyrexian_can_reserve_colored_mana_for_generic_and_pay_life() {
    check(
        vec![ManaSymbol::Generic(1), ManaSymbol::Phyrexian(Color::Green)],
        [0, 0, 0, 0, 1, 0],
        vec![],
        2,
    );
    check(
        vec![ManaSymbol::Generic(1), ManaSymbol::Phyrexian(Color::Green)],
        [0, 0, 0, 0, 1, 0],
        vec![],
        1,
    );
}
#[test]
fn mandatory_color_takes_precedence_while_phyrexian_is_paid_with_all_remaining_life() {
    check(
        vec![
            ManaSymbol::Colored(Color::Green),
            ManaSymbol::Phyrexian(Color::Green),
        ],
        [0, 0, 0, 0, 1, 0],
        vec![],
        2,
    );
}
#[test]
fn hybrid_color_choice_is_independent_of_other_mandatory_symbols() {
    check(
        vec![
            ManaSymbol::Hybrid(Color::White, Color::Blue),
            ManaSymbol::Colored(Color::White),
        ],
        [1, 1, 0, 0, 0, 0],
        vec![],
        0,
    );
}
#[test]
fn all_nine_monohybrid_symbols_may_use_generic_without_truncation() {
    check(
        vec![ManaSymbol::MonoHybrid(2, Color::White); 9],
        [0, 0, 0, 0, 0, 18],
        vec![],
        0,
    );
    check(
        vec![ManaSymbol::MonoHybrid(2, Color::White); 9],
        [0, 0, 0, 0, 0, 17],
        vec![],
        0,
    );
}
proptest! {
    #[test]
    fn independent_symbol_choices_match_production_feasibility_and_actual_payment(
        kinds in prop::collection::vec(0u8..8,0..=5),pool in prop::array::uniform6(0u16..=2),
        masks in prop::collection::vec(0u8..=31,0..=3),life in 0i32..=8,
    ) {
        let symbols=kinds.into_iter().map(|kind|match kind {
            0=>ManaSymbol::Generic(1),1=>ManaSymbol::Generic(2),2=>ManaSymbol::Colored(Color::Green),
            3=>ManaSymbol::Colorless,4=>ManaSymbol::Colored(Color::White),
            5=>ManaSymbol::Hybrid(Color::White,Color::Blue),6=>ManaSymbol::MonoHybrid(2,Color::White),
            _=>ManaSymbol::Phyrexian(Color::Green),
        }).collect();check(symbols,pool,masks,life);
    }
}

#[test]
fn monohybrid_count_exceeds_machine_word_without_truncation() {
    let requirements = mana::requirements(
        &ManaCost {
            symbols: vec![ManaSymbol::MonoHybrid(2, Color::White); 65],
        },
        0,
    );
    let payment = mana::plan_with(&requirements, &[], [0, 0, 0, 0, 0, 130], 0).unwrap();
    assert_eq!(payment.spend, [0, 0, 0, 0, 0, 130]);
    assert_eq!(payment.life, 0);
    assert!(mana::plan_with(&requirements, &[], [0, 0, 0, 0, 0, 129], 0).is_none());
}

#[test]
fn generic_and_multiple_phyrexian_symbols_compete_at_exact_life_boundary() {
    for life in 0..=6 {
        check(
            vec![
                ManaSymbol::Generic(2),
                ManaSymbol::Phyrexian(Color::Green),
                ManaSymbol::Phyrexian(Color::Green),
                ManaSymbol::Colored(Color::White),
            ],
            [1, 0, 0, 0, 3, 0],
            vec![],
            life,
        );
    }
}

#[test]
fn engine_pays_generic_with_green_and_phyrexian_with_life_before_resolution() {
    use mtg_core::{Event, PlayerId};
    use mtg_engine::{Action, ChoiceKind};
    use mtg_verify::scenario::{GameScenario, next_choice, run};
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/regressions");
    let mut scenario =
        GameScenario::load(&root.join("issue_local_mana_colorless_alternative.json")).unwrap();
    for action in &mut scenario.actions {
        action.expected_choice = None;
        action.expected_state = None;
        action.expected_digest = None;
    }
    scenario.expected.final_state = None;
    scenario.expected.final_digest = None;
    scenario.cards[0].abilities.truncate(1); // Only the unrestricted tap-for-G ability.
    scenario.cards[1].mana_cost.symbols =
        vec![ManaSymbol::Generic(1), ManaSymbol::Phyrexian(Color::Green)];
    scenario.expected.life.insert(PlayerId(0), 21); // 20 - 2 life cost + 3 life on resolution.
    scenario.metadata.issue = Some("local_mana_generic_phyrexian_competition".into());
    scenario.metadata.description = "One tap-for-G permanent pays the generic part of a 1 G/P instant; two life pays G/P. Casting leaves life eighteen and resolving gains three to twenty-one.".into();
    scenario.metadata.rules = vec!["107.4f".into(), "118.4".into(), "601.2h".into()];
    scenario.metadata.fixed_in =
        Some("working-tree reserve generic mana before optional phyrexian payment".into());
    scenario.card_db_hash = scenario.card_hash();
    let (mut engine, cards) = scenario.setup().unwrap();
    let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
    let ChoiceKind::Priority { legal } = choice.kind else {
        panic!("priority")
    };
    assert!(legal.actions.contains(&Action::Cast {
        object: ObjectId(2)
    }));
    for (index, action) in scenario.actions.iter().enumerate() {
        let choice = next_choice(&mut engine, &cards).unwrap().unwrap();
        engine
            .answer(&cards, choice.id, action.answer.clone())
            .unwrap();
        next_choice(&mut engine, &cards).unwrap();
        if index == 0 {
            assert_eq!(engine.state.player(PlayerId(0)).life, 18);
            assert!(engine.state.objects[&ObjectId(1)].tapped);
            assert_eq!(engine.state.player(PlayerId(0)).mana.amounts, [0; 6]);
            let deltas: Vec<_> = engine
                .log
                .iter()
                .filter_map(|entry| match entry.event {
                    Event::LifeChanged {
                        player: PlayerId(0),
                        delta,
                    } => Some(delta),
                    _ => None,
                })
                .collect();
            assert_eq!(deltas, vec![-2]);
            let added: Vec<_> = engine
                .log
                .iter()
                .filter_map(|entry| match entry.event {
                    Event::ManaAdded {
                        player: PlayerId(0),
                        color,
                        amount,
                    } => Some((color, amount)),
                    _ => None,
                })
                .collect();
            let spent: Vec<_> = engine
                .log
                .iter()
                .filter_map(|entry| match entry.event {
                    Event::ManaSpent {
                        player: PlayerId(0),
                        color,
                        amount,
                    } => Some((color, amount)),
                    _ => None,
                })
                .collect();
            assert_eq!(added, vec![(Some(Color::Green), 1)]);
            assert_eq!(spent, vec![(Some(Color::Green), 1)]);
        }
    }
    assert_eq!(engine.state.player(PlayerId(0)).life, 21);
    let (report, artifact) = run(&scenario, true).unwrap();
    assert!(report.pass, "{}", report.message);
    if std::env::var_os("MTGO_RECORD_ALTERNATIVE_FIXTURE").is_some() {
        use std::io::Write;
        std::fs::File::create_new(root.join("issue_local_mana_generic_phyrexian_competition.json"))
            .unwrap()
            .write_all(&serde_json::to_vec_pretty(&artifact).unwrap())
            .unwrap();
    }
}
