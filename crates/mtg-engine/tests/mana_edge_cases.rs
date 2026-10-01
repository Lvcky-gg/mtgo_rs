//! Edge case tests for mana resolution using the augmenting path algorithm.
//!
//! Complements `mana.rs` with complex scenarios involving 5+ colors,
//! hybrid mana sources, floating mana prioritization, and graceful failure.

use mtg_core::{AbilityId, Color, ManaCost, ManaSymbol, ObjectId};
use mtg_engine::mana::{ManaSource, plan_with, requirements};
use mtg_ir::effect::ManaOutput;

fn source(n: u32, output: ManaOutput) -> ManaSource {
    ManaSource {
        object: ObjectId(n),
        ability: AbilityId(0),
        outputs: vec![output],
        taps: true,
        sacrifices: false,
        only: None,
        spells_only: false,
    }
}

fn mono(n: u32, c: Color) -> ManaSource {
    source(n, ManaOutput::Colored(c))
}

fn dual(n: u32, a: Color, b: Color) -> ManaSource {
    source(n, ManaOutput::AnyOf(vec![a, b]))
}

fn tri(n: u32, colors: &[Color]) -> ManaSource {
    source(n, ManaOutput::AnyOf(colors.to_vec()))
}

fn cost(symbols: &[ManaSymbol]) -> ManaCost {
    ManaCost {
        symbols: symbols.to_vec(),
    }
}

const EMPTY: [u16; 6] = [0; 6];

fn payable(c: &ManaCost, sources: &[ManaSource], pool: [u16; 6], life: i32) -> bool {
    plan_with(&requirements(c, 0), sources, pool, life).is_some()
}

use Color::{Black as B, Blue as U, Green as G, Red as R, White as W};

// ---- 5+ color mana scenarios ----

#[test]
fn five_color_mana_with_perfect_sources() {
    // Each color has a dedicated source
    let sources = [mono(1, W), mono(2, U), mono(3, B), mono(4, R), mono(5, G)];
    let c = cost(&[
        ManaSymbol::Colored(W),
        ManaSymbol::Colored(U),
        ManaSymbol::Colored(B),
        ManaSymbol::Colored(R),
        ManaSymbol::Colored(G),
    ]);
    assert!(
        payable(&c, &sources, EMPTY, 20),
        "five-color cost is payable"
    );
}

#[test]
fn five_color_mana_with_duals_requires_matching() {
    // A complex scenario where dual lands must be routed correctly
    // Sources: W/U dual, U/B dual, B/R dual, R/G dual, G/W dual
    let sources = [
        dual(1, W, U),
        dual(2, U, B),
        dual(3, B, R),
        dual(4, R, G),
        dual(5, G, W),
    ];

    let c = cost(&[
        ManaSymbol::Colored(W),
        ManaSymbol::Colored(U),
        ManaSymbol::Colored(B),
        ManaSymbol::Colored(R),
        ManaSymbol::Colored(G),
    ]);
    // This forms a cycle; each dual land uniquely satisfies one color pair
    assert!(
        payable(&c, &sources, EMPTY, 20),
        "cyclic five-color duals should resolve"
    );
}

#[test]
fn five_color_impossible_scenario_fails() {
    // Only four sources, five colors needed
    let sources = [mono(1, W), mono(2, U), mono(3, B), mono(4, R)];
    let c = cost(&[
        ManaSymbol::Colored(W),
        ManaSymbol::Colored(U),
        ManaSymbol::Colored(B),
        ManaSymbol::Colored(R),
        ManaSymbol::Colored(G),
    ]);
    assert!(!payable(&c, &sources, EMPTY, 20), "insufficient sources");
}

#[test]
fn four_color_with_complex_duals() {
    // Four colors: W, U, B, R
    // Sources: W/U, W/B, U/R, B/R
    // Many valid assignments exist; the algorithm must find one
    let sources = [dual(1, W, U), dual(2, W, B), dual(3, U, R), dual(4, B, R)];
    let c = cost(&[
        ManaSymbol::Colored(W),
        ManaSymbol::Colored(U),
        ManaSymbol::Colored(B),
        ManaSymbol::Colored(R),
    ]);
    assert!(payable(&c, &sources, EMPTY, 20), "four-color complex duals");
}

// ---- Hybrid mana sources ----

#[test]
fn hybrid_source_correctly_routes_to_either_color() {
    let sources = [dual(1, W, U)];

    // {W} only needs white
    assert!(payable(
        &cost(&[ManaSymbol::Colored(W)]),
        &sources,
        EMPTY,
        20
    ));

    // {U} only needs blue
    assert!(payable(
        &cost(&[ManaSymbol::Colored(U)]),
        &sources,
        EMPTY,
        20
    ));
}

#[test]
fn multiple_hybrid_sources_serve_different_needs() {
    // Two W/U duals can pay {W}{U} - each dual produces one mana
    let sources = [dual(1, W, U), dual(2, W, U)];
    let c = cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(U)]);
    assert!(
        payable(&c, &sources, EMPTY, 20),
        "two duals can pay one white and one blue"
    );
}

#[test]
fn hybrid_with_mono_sources_for_single_color_requirement() {
    // Need {W}{U}
    // Have: W mono and U mono (can pay 2 colors)
    let sources = [mono(1, W), mono(2, U)];
    let c = cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(U)]);
    assert!(
        payable(&c, &sources, EMPTY, 20),
        "white and blue mono sources cover two colors"
    );
}

#[test]
fn hybrid_cannot_pay_same_color_twice_alone() {
    let sources = [dual(1, W, U)];
    // Only one dual land; cannot pay {W}{W}
    assert!(!payable(
        &cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(W)]),
        &sources,
        EMPTY,
        20
    ));
}

#[test]
fn three_color_hybrid_with_specific_routing() {
    // W/U/B source
    let sources = [tri(1, &[W, U, B])];

    // Can pay any single color
    assert!(payable(
        &cost(&[ManaSymbol::Colored(W)]),
        &sources,
        EMPTY,
        20
    ));
    assert!(payable(
        &cost(&[ManaSymbol::Colored(U)]),
        &sources,
        EMPTY,
        20
    ));
    assert!(payable(
        &cost(&[ManaSymbol::Colored(B)]),
        &sources,
        EMPTY,
        20
    ));

    // But not two different colors (only one mana)
    assert!(!payable(
        &cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(U)]),
        &sources,
        EMPTY,
        20
    ));
}

// ---- Floating mana prioritization ----

#[test]
fn floating_mana_spent_first_before_sources() {
    let sources = [mono(1, W), mono(2, W)];
    let mut pool = EMPTY;
    pool[W as usize] = 2;

    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Generic(2)]), 0),
        &sources,
        pool,
        20,
    )
    .expect("payable");

    assert!(
        plan.activate.is_empty(),
        "floating mana should be spent before tapping sources"
    );
}

#[test]
fn floating_mana_partially_satisfies_cost() {
    let sources = [mono(1, W), mono(2, W)];
    let mut pool = EMPTY;
    pool[W as usize] = 1;

    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Generic(3)]), 0),
        &sources,
        pool,
        20,
    )
    .expect("need 3 total, have 1 floating + 2 sources");

    assert_eq!(
        plan.activate.len(),
        2,
        "both sources tapped after spending floating mana"
    );
}

#[test]
fn colored_floating_mana_pays_colored_requirement() {
    let sources = [mono(1, W)];
    let mut pool = EMPTY;
    pool[W as usize] = 1;

    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Colored(W)]), 0),
        &sources,
        pool,
        20,
    )
    .expect("payable");

    assert!(
        plan.activate.is_empty(),
        "floating colored mana satisfies colored requirement"
    );
}

#[test]
fn floating_mana_wrong_color_needs_source() {
    let sources = [mono(1, W)];
    let mut pool = EMPTY;
    pool[U as usize] = 1; // Blue floating

    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Colored(W)]), 0),
        &sources,
        pool,
        20,
    )
    .expect("white source can pay white requirement");

    assert_eq!(
        plan.activate.len(),
        1,
        "floating wrong color doesn't help; tap the source"
    );
}

#[test]
fn floating_mana_pool_variety_is_prioritized() {
    // Pool has 1 white, 1 blue
    // Need {W}{U}
    // Should spend pool before sources
    let sources = [mono(1, W), mono(2, U)];
    let mut pool = EMPTY;
    pool[W as usize] = 1;
    pool[U as usize] = 1;

    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(U)]), 0),
        &sources,
        pool,
        20,
    )
    .expect("payable");

    assert!(
        plan.activate.is_empty(),
        "floating colored mana satisfies colored requirement"
    );
}

// ---- Least-flexible-source fallback ----

#[test]
fn least_flexible_source_prioritized_for_generic() {
    // Three sources: W/U dual, W mono, U mono
    // Paying {1} should tap the most-constrained mono source
    let sources = [dual(1, W, U), mono(2, W), mono(3, U)];

    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Generic(1)]), 0),
        &sources,
        EMPTY,
        20,
    )
    .expect("payable");

    // Should tap one of the mono sources, not the dual
    assert_eq!(plan.activate.len(), 1);
    let tapped = plan.activate[0].0;
    assert!(tapped == ObjectId(2) || tapped == ObjectId(3));
}

#[test]
fn flexibility_preserved_when_possible() {
    // Need {W}{G}
    // Have: W mono, W/G dual
    // Should use W mono for {W} and dual for {G}
    let sources = [mono(1, W), dual(2, W, G)];

    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(G)]), 0),
        &sources,
        EMPTY,
        20,
    )
    .expect("payable");

    assert_eq!(plan.activate.len(), 2, "both sources needed");
    // Verify that the W mono is assigned to {W}
    assert!(
        plan.activate
            .contains(&(ObjectId(1), AbilityId(0), Some(W)))
    );
}

// ---- Impossible scenarios and error cases ----

#[test]
fn impossible_cost_returns_none() {
    let sources = [mono(1, W)];
    let result = plan_with(
        &requirements(&cost(&[ManaSymbol::Colored(U)]), 0),
        &sources,
        EMPTY,
        20,
    );
    assert!(result.is_none(), "cannot pay blue with only white source");
}

#[test]
fn insufficient_sources_fails_gracefully() {
    let sources = [mono(1, W)];
    let result = plan_with(
        &requirements(&cost(&[ManaSymbol::Generic(2)]), 0),
        &sources,
        EMPTY,
        20,
    );
    assert!(
        result.is_none(),
        "cannot pay generic 2 with only one white source"
    );
}

#[test]
fn no_sources_rejects_all_costs() {
    let sources: [ManaSource; 0] = [];
    assert!(
        plan_with(
            &requirements(&cost(&[ManaSymbol::Generic(1)]), 0),
            &sources,
            EMPTY,
            20
        )
        .is_none()
    );
    assert!(
        plan_with(
            &requirements(&cost(&[ManaSymbol::Colored(W)]), 0),
            &sources,
            EMPTY,
            20
        )
        .is_none()
    );
}

#[test]
fn phyrexian_impossible_with_low_life() {
    // {W/P} needs either white or 2 life
    let sources: [ManaSource; 0] = [];
    let result = plan_with(
        &requirements(&cost(&[ManaSymbol::Phyrexian(W)]), 0),
        &sources,
        EMPTY,
        1, // Only 1 life
    );
    assert!(
        result.is_none(),
        "cannot pay phyrexian without sufficient life"
    );
}

#[test]
fn phyrexian_prefers_mana_over_life() {
    let sources = [mono(1, W)];
    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Phyrexian(W)]), 0),
        &sources,
        EMPTY,
        20,
    )
    .expect("payable with mana");

    assert_eq!(plan.life, 0, "should not pay life when mana available");
    assert_eq!(plan.total_mana(), 1);
}

#[test]
fn complex_hybrid_impossible_scenario() {
    // Need {W/U}{W/B}{U/B}
    // Have only {W/U} dual
    // Impossible: can only produce one mana, need three
    let sources = [dual(1, W, U)];
    let c = cost(&[
        ManaSymbol::Hybrid(W, U),
        ManaSymbol::Hybrid(W, B),
        ManaSymbol::Hybrid(U, B),
    ]);
    assert!(
        !payable(&c, &sources, EMPTY, 20),
        "one dual cannot pay three hybrid costs"
    );
}

#[test]
fn complex_scenario_five_colors_specific_routing() {
    // A sophisticated scenario combining many constraints:
    // Need: {W}{U}{B}{R}{G}
    // Have: 5 sources, each covering combinations
    let sources = [mono(1, W), mono(2, U), mono(3, B), mono(4, R), mono(5, G)];

    let c = cost(&[
        ManaSymbol::Colored(W),
        ManaSymbol::Colored(U),
        ManaSymbol::Colored(B),
        ManaSymbol::Colored(R),
        ManaSymbol::Colored(G),
    ]);

    // With perfect mono sources, this should easily resolve
    assert!(
        payable(&c, &sources, EMPTY, 20),
        "five mono sources can pay five-color cost"
    );
}

#[test]
fn variable_cost_with_complex_colors() {
    // X=1, cost is {X}{W}{U}
    let sources = [mono(1, W), mono(2, U), mono(3, W)];

    let c = cost(&[
        ManaSymbol::Variable,
        ManaSymbol::Colored(W),
        ManaSymbol::Colored(U),
    ]);

    let plan = plan_with(&requirements(&c, 1), &sources, EMPTY, 20).expect("X=1 plus two colors");
    // Should tap all 3 sources: 1 for X, 2 for colors
    assert_eq!(plan.activate.len(), 3);
}
