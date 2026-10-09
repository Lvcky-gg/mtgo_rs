//! Independent enumeration of whole activations: fixed output vectors or one
//! chosen color repeated. No hybrid, restricted, sacrifice or untap abilities.
use mtg_core::{AbilityId, Color, ManaCost, ManaSymbol, ObjectId};
use mtg_engine::mana::{self, ManaSource};
use mtg_ir::{ManaOutput, Value};
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
enum Output {
    Fixed(Vec<usize>),
    Uniform(u8, u8),
}
fn vector(slots: &[usize]) -> [u16; 6] {
    let mut out = [0; 6];
    for slot in slots {
        out[*slot] += 1;
    }
    out
}
fn alternatives(output: &Output) -> Vec<[u16; 6]> {
    match output {
        Output::Fixed(slots) => vec![vector(slots)],
        Output::Uniform(mask, count) => (0..5)
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| {
                let mut v = [0; 6];
                v[i] = u16::from(*count);
                v
            })
            .collect(),
    }
}
fn oracle(groups: &[Vec<Output>], available: [u16; 6], required: [u16; 6], generic: u8) -> bool {
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
        for produced in alternatives(ability) {
            if oracle(
                &groups[1..],
                std::array::from_fn(|i| available[i] + produced[i]),
                required,
                generic,
            ) {
                return true;
            }
        }
    }
    false
}
fn sources(groups: &[Vec<Output>]) -> Vec<ManaSource> {
    groups
        .iter()
        .enumerate()
        .flat_map(|(o, abilities)| {
            abilities
                .iter()
                .enumerate()
                .map(move |(a, output)| ManaSource {
                    object: ObjectId(o as u32 + 1),
                    ability: AbilityId(a as u16),
                    taps: true,
                    sacrifices: false,
                    only: None,
                    spells_only: false,
                    outputs: match output {
                        Output::Fixed(slots) => slots
                            .iter()
                            .map(|i| {
                                if *i == 5 {
                                    ManaOutput::Colorless
                                } else {
                                    ManaOutput::Colored(COLORS[*i])
                                }
                            })
                            .collect(),
                        Output::Uniform(mask, count) => vec![ManaOutput::Repeated {
                            amount: Value::Fixed(i32::from(*count)),
                            output: Box::new(ManaOutput::AnyOf(
                                (0..5)
                                    .filter(|i| mask & (1 << i) != 0)
                                    .map(|i| COLORS[i])
                                    .collect(),
                            )),
                        }],
                    },
                })
        })
        .collect()
}
fn check(groups: Vec<Vec<Output>>, pool: [u16; 6], required: [u16; 6], generic: u8) {
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
    let sources = sources(&groups);
    let plan = mana::plan_with(
        &mana::requirements(&ManaCost { symbols }, 0),
        &sources,
        pool,
        20,
    );
    assert_eq!(
        plan.is_some(),
        oracle(&groups, pool, required, generic),
        "groups={groups:?} pool={pool:?} required={required:?} generic={generic}"
    );
    if let Some(plan) = plan {
        assert_eq!(plan.life, 0);
        let mut available = pool;
        let mut used = BTreeSet::new();
        for (object, ability, color) in plan.activate {
            assert!(
                used.insert(object),
                "one physical permanent cannot activate two tap abilities"
            );
            assert!(
                sources
                    .iter()
                    .any(|s| s.object == object && s.ability == ability)
            );
            let output = &groups[object.0 as usize - 1][ability.0 as usize];
            let produced = match output {
                Output::Fixed(slots) => vector(slots),
                Output::Uniform(mask, count) => {
                    let c = color.expect("uniform colored output selects one color") as usize;
                    assert!(mask & (1 << c) != 0);
                    let mut v = [0; 6];
                    v[c] = u16::from(*count);
                    v
                }
            };
            for slot in 0..6 {
                available[slot] += produced[slot];
            }
        }
        let mut generic_paid = 0;
        for slot in 0..6 {
            assert!(
                plan.spend[slot] <= available[slot],
                "activation cannot produce claimed mana"
            );
            assert!(plan.spend[slot] >= required[slot]);
            generic_paid += plan.spend[slot] - required[slot];
        }
        assert_eq!(generic_paid, u16::from(generic));
    }
}
#[test]
fn more_productive_green_ability_does_not_hide_colorless_alternative() {
    check(
        vec![vec![Output::Fixed(vec![4, 4]), Output::Fixed(vec![5])]],
        [0; 6],
        [0, 0, 0, 0, 0, 1],
        0,
    );
}
#[test]
fn two_mana_of_one_chosen_color_cannot_pay_white_and_blue() {
    check(
        vec![vec![Output::Uniform(3, 2)]],
        [0; 6],
        [1, 1, 0, 0, 0, 0],
        0,
    );
}
#[test]
fn fixed_white_blue_vector_remains_available_with_green_alternative() {
    check(
        vec![vec![Output::Fixed(vec![0, 1]), Output::Fixed(vec![4])]],
        [0; 6],
        [1, 1, 0, 0, 0, 0],
        0,
    );
}
#[test]
fn feasible_payment_must_activate_the_other_source_for_second_color() {
    // Feasible via BB from object1 plus R from object2; recording only one
    // mixed-color repeated activation cannot justify the claimed B+R spend.
    check(
        vec![
            vec![Output::Fixed(vec![2]), Output::Uniform(12, 2)],
            vec![Output::Uniform(9, 1)],
            vec![Output::Uniform(1, 1)],
        ],
        [1, 1, 0, 0, 0, 1],
        [0, 0, 1, 1, 0, 0],
        0,
    );
}

proptest! {
    #[test]
    fn whole_activation_enumeration_matches_production(
        groups in prop::collection::vec(prop::collection::vec(prop_oneof![prop::collection::vec(0usize..=5,1..=3).prop_map(Output::Fixed),(1u8..=31,1u8..=3).prop_map(|(mask,count)|Output::Uniform(mask,count))],1..=2),0..=3),
        pool in prop::array::uniform6(0u16..=1),required in prop::array::uniform6(0u16..=1),generic in 0u8..=3,
    ) {check(groups,pool,required,generic);}
}

#[test]
fn impossible_generic_cost_prunes_many_correlated_sources() {
    // Independent upper bound:32 activations times2 mana cannot pay65, whatever
    // uniform colors are chosen. Exhaustively visiting2^32 branches is needless.
    let grouped = vec![vec![Output::Uniform(3, 2)]; 32];
    let sources = sources(&grouped);
    let requirement = mana::requirements(
        &ManaCost {
            symbols: vec![ManaSymbol::Generic(65)],
        },
        0,
    );
    let started = std::time::Instant::now();
    assert!(mana::plan_with(&requirement, &sources, [0; 6], 20).is_none());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "capacity impossibility should avoid exponential branch search"
    );
}

#[test]
fn capacity_bound_does_not_count_life_payable_phyrexian_symbols_as_mana_required() {
    let sources = sources(&[vec![Output::Uniform(3, 2)]]);
    let requirement = mana::requirements(
        &ManaCost {
            symbols: vec![ManaSymbol::Phyrexian(Color::Green); 4],
        },
        0,
    );
    let payment = mana::plan_with(&requirement, &sources, [0; 6], 8)
        .expect("four phyrexian green symbols can be paid with eight life");
    assert_eq!(payment.life, 8);
    assert_eq!(payment.spend, [0; 6]);
    assert!(payment.activate.is_empty());
    assert!(mana::plan_with(&requirement, &sources, [0; 6], 7).is_none());
}
