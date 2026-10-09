use mtg_core::{AbilityId, Color, ManaCost, ManaSymbol, ObjectId};
use mtg_engine::mana::{self, ManaSource};
use mtg_ir::ManaOutput;
use mtg_verify::{
    differential::compare,
    mana_reference::{ManaOracle, ManaProblem},
};
use proptest::prelude::*;

const COLORS: [Color; 5] = [
    Color::White,
    Color::Blue,
    Color::Black,
    Color::Red,
    Color::Green,
];

fn source_choices(mask: u8) -> Vec<Option<Color>> {
    if mask == 0 {
        vec![None]
    } else {
        COLORS
            .iter()
            .enumerate()
            .filter_map(|(i, color)| ((mask & (1 << i)) != 0).then_some(Some(*color)))
            .collect()
    }
}
fn sources(problem: &ManaProblem) -> Vec<ManaSource> {
    problem
        .sources
        .iter()
        .enumerate()
        .map(|(i, choices)| ManaSource {
            object: ObjectId(i as u32 + 1),
            ability: AbilityId(0),
            outputs: vec![if choices == &[None] {
                ManaOutput::Colorless
            } else {
                ManaOutput::AnyOf(choices.iter().flatten().copied().collect())
            }],
            taps: true,
            sacrifices: false,
            only: None,
            spells_only: false,
        })
        .collect()
}
fn cost(problem: &ManaProblem) -> ManaCost {
    let mut symbols = vec![ManaSymbol::Generic(problem.generic as u8)];
    for (slot, &amount) in problem.required.iter().enumerate() {
        for _ in 0..amount {
            symbols.push(if slot == 5 {
                ManaSymbol::Colorless
            } else {
                ManaSymbol::Colored(COLORS[slot])
            });
        }
    }
    ManaCost { symbols }
}

fn check(problem: &ManaProblem) {
    let sources = sources(problem);
    let payment = mana::plan_with(
        &mana::requirements(&cost(problem), 0),
        &sources,
        problem.pool,
        20,
    );
    let comparison = compare(&ManaOracle, problem, Ok(payment.is_some()));
    assert!(
        comparison.is_ok(),
        "problem: {problem:?}; disagreement: {comparison:?}"
    );
    if let Some(payment) = payment {
        // Validate the plan independently of the planner and matching internals.
        assert_eq!(payment.life, 0);
        let mut available = problem.pool.map(u32::from);
        let mut used = std::collections::BTreeSet::new();
        for (object, ability, color) in payment.activate {
            assert!(used.insert(object), "one source cannot pay twice");
            assert_eq!(ability, AbilityId(0));
            let index = object.0 as usize - 1;
            assert!(problem.sources[index].contains(&color));
            available[color.map_or(5, |color| color as usize)] += 1;
        }
        let mut generic_spent = 0;
        for (slot, &spend) in payment.spend.iter().enumerate() {
            assert!(
                u32::from(spend) <= available[slot],
                "spending unavailable mana"
            );
            assert!(
                spend >= problem.required[slot],
                "missing exact-color payment"
            );
            generic_spent += u32::from(spend - problem.required[slot]);
        }
        assert_eq!(generic_spent, u32::from(problem.generic));
    }
}

#[test]
fn exact_generic_boundary_and_flexible_source_no_double_spend() {
    check(&ManaProblem {
        pool: [0, 0, 0, 0, 0, 2],
        required: [0; 6],
        generic: 2,
        sources: vec![],
    });
    check(&ManaProblem {
        pool: [0; 6],
        required: [1, 1, 0, 0, 0, 0],
        generic: 0,
        sources: vec![vec![Some(Color::White), Some(Color::Blue)]],
    });
    check(&ManaProblem {
        pool: [0; 6],
        required: [1, 1, 0, 0, 0, 0],
        generic: 0,
        sources: vec![vec![Some(Color::White), Some(Color::Blue)]; 2],
    });
}
#[test]
fn colorless_requires_actual_colorless_and_extra_mana_is_not_creation() {
    check(&ManaProblem {
        pool: [5, 0, 0, 0, 0, 0],
        required: [0, 0, 0, 0, 0, 1],
        generic: 0,
        sources: vec![],
    });
    check(&ManaProblem {
        pool: [0; 6],
        required: [0, 0, 0, 0, 0, 1],
        generic: 0,
        sources: vec![vec![None]],
    });
}
#[test]
fn reference_rejects_unmodeled_or_unbounded_inputs() {
    let mut input = ManaProblem {
        pool: [0; 6],
        required: [0; 6],
        generic: 0,
        sources: vec![vec![None, Some(Color::Blue)]],
    };
    assert!(input.feasible().is_err());
    input.sources = vec![vec![None]; 7];
    assert!(input.feasible().is_err());
}
proptest! {
    #[test]
    fn production_mana_plan_matches_independent_exhaustive_color_assignments(
        pool in prop::array::uniform6(0u16..=3),
        required in prop::array::uniform6(0u16..=2),
        generic in 0u16..=6,
        masks in prop::collection::vec(0u8..=31, 0..6),
    ) {
        check(&ManaProblem { pool, required, generic, sources: masks.into_iter().map(source_choices).collect() });
    }
}
