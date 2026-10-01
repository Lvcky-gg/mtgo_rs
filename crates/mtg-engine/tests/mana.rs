//! The mana availability algorithm.
//!
//! These go straight at `plan_with`, which takes sources and a pool rather than a
//! `GameState` — the algorithm was deliberately separated from state so it could be
//! tested without building a board.

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

/// A land producing exactly one colour.
fn mono(n: u32, c: Color) -> ManaSource {
    source(n, ManaOutput::Colored(c))
}

/// A land producing one mana, of either of two colours.
fn dual(n: u32, a: Color, b: Color) -> ManaSource {
    source(n, ManaOutput::AnyOf(vec![a, b]))
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

use Color::{Blue as U, Green as G, White as W};

// ---- the case a per-colour count gets wrong -----------------------------

#[test]
fn one_dual_land_cannot_pay_two_different_colours() {
    // This is the whole reason for the matching. A per-colour count sees this land as
    // one white available *and* one blue available, so {W}{U} looks payable — but it
    // makes a single mana, and spending it on white leaves nothing for blue.
    let sources = [dual(1, W, U)];
    assert!(
        !payable(
            &cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(U)]),
            &sources,
            EMPTY,
            20
        ),
        "one dual land must not pay {{W}}{{U}}"
    );
}

#[test]
fn two_dual_lands_do_pay_two_of_the_same_colour() {
    // The converse, and just as important: each land independently chooses white, so
    // this is payable. An implementation that capped each colour by the number of
    // sources dedicated to it would wrongly reject this.
    let sources = [dual(1, W, U), dual(2, W, U)];
    assert!(payable(
        &cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(W)]),
        &sources,
        EMPTY,
        20
    ));
}

#[test]
fn one_dual_land_cannot_pay_a_colour_twice_either() {
    let sources = [dual(1, W, U)];
    assert!(!payable(
        &cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(W)]),
        &sources,
        EMPTY,
        20
    ));
}

#[test]
fn two_dual_lands_can_pay_one_of_each_colour() {
    let sources = [dual(1, W, U), dual(2, W, U)];
    assert!(payable(
        &cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(U)]),
        &sources,
        EMPTY,
        20
    ));
}

#[test]
fn a_constrained_source_forces_the_flexible_one_to_move() {
    // The augmenting-path case. One land makes only W; the other makes W or U. A
    // first-come assignment could give the dual land to {W} and then fail on {U};
    // the matching has to rehouse it.
    let sources = [mono(1, W), dual(2, W, U)];
    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(U)]), 0),
        &sources,
        EMPTY,
        20,
    )
    .expect("should be payable by rehousing the dual land onto blue");
    assert_eq!(plan.activate.len(), 2, "both lands are tapped");
    assert_eq!(plan.total_mana(), 2);
}

#[test]
fn a_chain_of_constraints_still_resolves() {
    // W-only, W-or-U, U-or-G, paying {W}{U}{G}: every source has to shift one place.
    let sources = [mono(1, W), dual(2, W, U), dual(3, U, G)];
    assert!(payable(
        &cost(&[
            ManaSymbol::Colored(W),
            ManaSymbol::Colored(U),
            ManaSymbol::Colored(G)
        ]),
        &sources,
        EMPTY,
        20
    ));
}

// ---- generic, colorless, hybrid ----------------------------------------

#[test]
fn leftover_sources_pay_the_generic_part() {
    let sources = [mono(1, W), mono(2, W), mono(3, W)];
    assert!(payable(
        &cost(&[ManaSymbol::Colored(W), ManaSymbol::Generic(2)]),
        &sources,
        EMPTY,
        20
    ));
    // One short.
    assert!(!payable(
        &cost(&[ManaSymbol::Colored(W), ManaSymbol::Generic(3)]),
        &sources,
        EMPTY,
        20
    ));
}

#[test]
fn any_colour_of_mana_can_pay_generic() {
    let sources = [mono(1, W), mono(2, G)];
    assert!(payable(
        &cost(&[ManaSymbol::Generic(2)]),
        &sources,
        EMPTY,
        20
    ));
}

#[test]
fn colorless_specifically_cannot_be_paid_with_coloured_mana() {
    // {C} is not the same as {1}.
    let coloured = [mono(1, W)];
    assert!(!payable(
        &cost(&[ManaSymbol::Colorless]),
        &coloured,
        EMPTY,
        20
    ));

    let colorless = [source(1, ManaOutput::Colorless)];
    assert!(payable(
        &cost(&[ManaSymbol::Colorless]),
        &colorless,
        EMPTY,
        20
    ));
}

#[test]
fn colorless_mana_still_pays_generic() {
    let sources = [source(1, ManaOutput::Colorless)];
    assert!(payable(
        &cost(&[ManaSymbol::Generic(1)]),
        &sources,
        EMPTY,
        20
    ));
}

#[test]
fn a_hybrid_symbol_accepts_either_half() {
    let with_white = [mono(1, W)];
    let with_blue = [mono(1, U)];
    let with_green = [mono(1, G)];
    let c = cost(&[ManaSymbol::Hybrid(W, U)]);
    assert!(payable(&c, &with_white, EMPTY, 20));
    assert!(payable(&c, &with_blue, EMPTY, 20));
    assert!(
        !payable(&c, &with_green, EMPTY, 20),
        "green satisfies neither half"
    );
}

#[test]
fn a_mono_hybrid_symbol_falls_back_to_generic() {
    // {2/W}: one white, or two of anything.
    let one_white = [mono(1, W)];
    assert!(payable(
        &cost(&[ManaSymbol::MonoHybrid(2, W)]),
        &one_white,
        EMPTY,
        20
    ));

    let two_green = [mono(1, G), mono(2, G)];
    assert!(
        payable(
            &cost(&[ManaSymbol::MonoHybrid(2, W)]),
            &two_green,
            EMPTY,
            20
        ),
        "two of any colour should pay the generic half"
    );

    let one_green = [mono(1, G)];
    assert!(!payable(
        &cost(&[ManaSymbol::MonoHybrid(2, W)]),
        &one_green,
        EMPTY,
        20
    ));
}

// ---- phyrexian and life ------------------------------------------------

#[test]
fn a_phyrexian_symbol_can_be_paid_with_life() {
    let none = [];
    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Phyrexian(W)]), 0),
        &none,
        EMPTY,
        20,
    )
    .expect("payable with life");
    assert_eq!(plan.life, 2);
    assert_eq!(plan.total_mana(), 0);
}

#[test]
fn a_phyrexian_symbol_prefers_mana_when_it_is_available() {
    let sources = [mono(1, W)];
    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Phyrexian(W)]), 0),
        &sources,
        EMPTY,
        20,
    )
    .expect("payable");
    assert_eq!(plan.life, 0, "should not pay life when mana will do");
    assert_eq!(plan.total_mana(), 1);
}

#[test]
fn life_cannot_be_paid_beyond_the_life_total() {
    // CR 118.4.
    let none = [];
    assert!(
        plan_with(
            &requirements(&cost(&[ManaSymbol::Phyrexian(W)]), 0),
            &none,
            EMPTY,
            1
        )
        .is_none()
    );
}

// ---- the pool ----------------------------------------------------------

#[test]
fn floating_mana_pays_before_anything_is_tapped() {
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
        "mana already in the pool should be spent before tapping a land"
    );
}

#[test]
fn the_pool_and_sources_combine() {
    let sources = [mono(1, W)];
    let mut pool = EMPTY;
    pool[W as usize] = 1;
    assert!(payable(
        &cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(W)]),
        &sources,
        pool,
        20
    ));
}

// ---- X and empty costs -------------------------------------------------

#[test]
fn an_x_spell_is_payable_with_x_at_zero() {
    let sources = [mono(1, W)];
    assert!(payable(
        &cost(&[ManaSymbol::Variable, ManaSymbol::Colored(W)]),
        &sources,
        EMPTY,
        20
    ));
}

#[test]
fn x_consumes_extra_mana_when_announced() {
    let sources = [mono(1, W), mono(2, W), mono(3, W)];
    let c = cost(&[ManaSymbol::Variable, ManaSymbol::Colored(W)]);
    assert!(
        plan_with(&requirements(&c, 2), &sources, EMPTY, 20).is_some(),
        "X=2 needs three"
    );
    assert!(
        plan_with(&requirements(&c, 3), &sources, EMPTY, 20).is_none(),
        "X=3 needs four"
    );
}

#[test]
fn a_free_cost_needs_nothing() {
    let none = [];
    assert!(payable(&ManaCost::FREE, &none, EMPTY, 20));
}

#[test]
fn a_source_producing_two_mana_counts_twice() {
    let two = [ManaSource {
        object: ObjectId(1),
        ability: AbilityId(0),
        outputs: vec![ManaOutput::Colored(W), ManaOutput::Colored(W)],
        taps: true,
        sacrifices: false,
        only: None,
        spells_only: false,
    }];
    assert!(payable(
        &cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(W)]),
        &two,
        EMPTY,
        20
    ));
    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Colored(W), ManaSymbol::Colored(W)]), 0),
        &two,
        EMPTY,
        20,
    )
    .unwrap();
    assert_eq!(
        plan.activate.len(),
        1,
        "one permanent is tapped, producing two mana"
    );
}

#[test]
fn a_tapped_out_board_pays_nothing() {
    let none = [];
    assert!(!payable(&cost(&[ManaSymbol::Colored(W)]), &none, EMPTY, 20));
    assert!(!payable(&cost(&[ManaSymbol::Generic(1)]), &none, EMPTY, 20));
}

// ---- auto-tapping leaves the flexible sources for later -----------------

#[test]
fn generic_mana_is_paid_from_the_least_flexible_source() {
    // Tapping the dual for {1} would leave only white, and strand a green spell cast next.
    let sources = [dual(1, W, G), mono(2, W)];
    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Generic(1)]), 0),
        &sources,
        EMPTY,
        20,
    )
    .unwrap();
    assert_eq!(plan.activate.len(), 1);
    assert_eq!(
        plan.activate[0].0,
        ObjectId(2),
        "the basic is tapped, the dual kept"
    );
}

#[test]
fn a_coloured_symbol_is_paid_from_the_least_flexible_source_that_fits() {
    let sources = [dual(1, W, G), mono(2, W)];
    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Colored(W)]), 0),
        &sources,
        EMPTY,
        20,
    )
    .unwrap();
    assert_eq!(plan.activate[0].0, ObjectId(2));
}

#[test]
fn flexibility_ordering_never_costs_a_payment_that_exists() {
    // Ordering is a preference, not a restriction: when only the dual can make green, it
    // is still used for green.
    let sources = [dual(1, W, G), mono(2, W)];
    let plan = plan_with(
        &requirements(&cost(&[ManaSymbol::Colored(G), ManaSymbol::Colored(W)]), 0),
        &sources,
        EMPTY,
        20,
    )
    .unwrap();
    assert_eq!(plan.activate.len(), 2);
    assert!(
        plan.activate
            .contains(&(ObjectId(1), AbilityId(0), Some(G)))
    );
}

// ---- choices offered when tapping by hand -------------------------------

#[test]
fn a_one_colour_source_offers_a_single_activation() {
    assert_eq!(mtg_engine::mana::color_options(&mono(1, W)), vec![None]);
}

#[test]
fn a_dual_source_offers_one_activation_per_colour() {
    assert_eq!(
        mtg_engine::mana::color_options(&dual(1, W, G)),
        vec![Some(W), Some(G)]
    );
}
