//! "{T}: Add three mana in any combination of {R} and/or {G}.": one activation, each unit
//! the color the payment needs. (Orcish Lumberjack's "Sacrifice a Forest" cost isn't
//! planned yet, so its line is refused rather than played with one color.)

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

const SOURCE: &str = "{T}: Add three mana in any combination of {R} and/or {G}.";

#[test]
fn one_activation_pays_for_a_spell_needing_both_colors() {
    let mut t = Table::default();
    let source = t.card("{R}", "Creature — Orc", Some((1, 1)), SOURCE);
    let beast = t.card("{R}{G}{G}", "Creature — Beast", Some((4, 4)), "");
    let mut g = Game::new(t);
    g.put(source, P0, Zone::Battlefield);
    let spell = g.put(beast, P0, Zone::Hand);
    assert!(g.main().contains(&Action::Cast { object: spell }));
    g.cast(spell, &[]);
    assert!(g.find(beast).is_some(), "the beast resolved");
}

#[test]
fn three_is_not_four() {
    let mut t = Table::default();
    let source = t.card("{R}", "Creature — Orc", Some((1, 1)), SOURCE);
    let giant = t.card("{2}{R}{G}", "Creature — Giant", Some((5, 5)), "");
    let mut g = Game::new(t);
    g.put(source, P0, Zone::Battlefield);
    let spell = g.put(giant, P0, Zone::Hand);
    assert!(!g.main().contains(&Action::Cast { object: spell }));
}

#[test]
fn a_sacrifice_another_cost_is_refused_rather_than_made_one_color() {
    let parsed = mtg_oracle::compile::compile(
        &mtg_oracle::compile::FaceText {
            name: "Lumberjack",
            card_types: &[mtg_core::CardType::Creature],
            subtypes: &[],
            oracle_text: Some(
                "{T}, Sacrifice a Forest: Add three mana in any combination of {R} and/or {G}.",
            ),
            mana_cost: "{R}",
        },
        &mtg_oracle::compile::SubtypeNames(vec![(3, "Forest".into())]),
    );
    assert!(!parsed.understood());
}

#[test]
fn any_combination_of_colors_splits_among_all_five() {
    let mut t = Table::default();
    let source = t.card(
        "{2}",
        "Artifact",
        None,
        "{T}: Add two mana in any combination of colors.",
    );
    let spirit = t.card("{W}{B}", "Creature — Spirit", Some((2, 2)), "");
    let mut g = Game::new(t);
    g.put(source, P0, Zone::Battlefield);
    let spell = g.put(spirit, P0, Zone::Hand);
    assert!(g.main().contains(&Action::Cast { object: spell }));
    g.cast(spell, &[]);
    assert!(g.find(spirit).is_some(), "the spirit resolved");
}

// The payment planner doesn't plan a source that itself costs mana (true of "{1}, {T}: Add
// {W}{B}." too), so it is activated by hand: both units the color chosen.
#[test]
fn an_activation_with_a_mana_cost_is_made_by_hand() {
    let mut t = Table::default();
    let source = t.card(
        "{2}",
        "Artifact",
        None,
        "{1}, {T}: Add two mana in any combination of colors.",
    );
    let land = t.card("", "Land", None, "{T}: Add {C}.");
    let mut g = Game::new(t);
    let source = g.put(source, P0, Zone::Battlefield);
    g.put(land, P0, Zone::Battlefield);
    g.main();
    let mana = Action::ActivateManaAbility {
        source,
        ability: mtg_core::AbilityId(0),
        color: Some(mtg_core::Color::Blue),
    };
    g.act(mana, &[], &[]);
    let pool = &g.engine.state.player(P0).mana;
    assert_eq!(pool.total(), 2, "{pool:?}");
}

#[test]
fn a_spending_restriction_holds_for_every_unit() {
    let mut t = Table::default();
    let source = t.card(
        "{2}",
        "Artifact",
        None,
        "{T}: Add two mana in any combination of colors. Spend this mana only to cast legendary spells.",
    );
    let plain = t.card("{W}{B}", "Creature — Spirit", Some((2, 2)), "");
    let legend = t.card("{W}{B}", "Legendary Creature — Spirit", Some((2, 2)), "");
    let mut g = Game::new(t);
    g.put(source, P0, Zone::Battlefield);
    let plain = g.put(plain, P0, Zone::Hand);
    let legend = g.put(legend, P0, Zone::Hand);
    let actions = g.main();
    assert!(!actions.contains(&Action::Cast { object: plain }));
    assert!(actions.contains(&Action::Cast { object: legend }));
    g.cast(legend, &[]);
    assert_eq!(g.stack().len(), 0);
}
