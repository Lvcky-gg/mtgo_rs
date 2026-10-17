//! CR 305.7 — "Nonbasic lands are Mountains." (Blood Moon): a nonbasic land loses its
//! abilities and land types and taps for {R}. CR 305.6 — "Each land is a Forest in addition
//! to its other land types." (Yavimaya, Cradle of Growth): it also taps for {G}.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;
use mtg_ir::PrintedCards;

#[test]
fn blood_moon_makes_a_nonbasic_land_a_mountain() {
    let mut t = Table::default();
    let moon = t.card(
        "{2}{R}",
        "Enchantment",
        None,
        "Nonbasic lands are Mountains.",
    );
    let tower = t.card("", "Land — Island Swamp", None, "{T}: Add {U} or {B}.");
    let blue = t.card("{U}", "Creature — Bird", Some((1, 1)), "");
    let red = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.put(moon, P1, Zone::Battlefield);
    let land = g.put(tower, P0, Zone::Battlefield);
    let blue = g.put(blue, P0, Zone::Hand);
    let red = g.put(red, P0, Zone::Hand);
    let actions = g.main();
    assert!(!actions.contains(&Action::Cast { object: blue }));
    assert!(actions.contains(&Action::Cast { object: red }));
    let ch = g.engine.characteristics(&g.table, land).unwrap();
    let mountain = g.table.subtype_named("Mountain").unwrap();
    let island = g.table.subtype_named("Island").unwrap();
    assert!(ch.subtypes.contains(&mountain));
    assert!(!ch.subtypes.contains(&island));
    let goblin = g.engine.state.objects[&red].card;
    g.cast(red, &[]);
    assert!(g.find(goblin).is_some(), "paid with the Mountain's {{R}}");
}

#[test]
fn blood_moon_leaves_basic_lands_alone() {
    let mut t = Table::default();
    let moon = t.card(
        "{2}{R}",
        "Enchantment",
        None,
        "Nonbasic lands are Mountains.",
    );
    let island = t.card("", "Basic Land — Island", None, "");
    let blue = t.card("{U}", "Creature — Bird", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.put(moon, P1, Zone::Battlefield);
    g.put(island, P0, Zone::Battlefield);
    let blue = g.put(blue, P0, Zone::Hand);
    assert!(g.main().contains(&Action::Cast { object: blue }));
}

#[test]
fn yavimaya_lets_a_plains_tap_for_green() {
    let mut t = Table::default();
    let yavimaya = t.card(
        "",
        "Legendary Land",
        None,
        "Each land is a Forest in addition to its other land types.",
    );
    let plains = t.card("", "Basic Land — Plains", None, "");
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let white = t.card("{W}", "Creature — Cat", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.put(yavimaya, P1, Zone::Battlefield);
    let land = g.put(plains, P0, Zone::Battlefield);
    let elf = g.put(elf, P0, Zone::Hand);
    let white = g.put(white, P0, Zone::Hand);
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object: elf }));
    assert!(
        actions.contains(&Action::Cast { object: white }),
        "still a Plains"
    );
    let ch = g.engine.characteristics(&g.table, land).unwrap();
    assert!(
        ch.subtypes
            .contains(&g.table.subtype_named("Forest").unwrap())
    );
    g.cast(elf, &[]);
}

#[test]
fn under_blood_moon_a_tapland_enters_untapped() {
    let mut t = Table::default();
    let moon = t.card(
        "{2}{R}",
        "Enchantment",
        None,
        "Nonbasic lands are Mountains.",
    );
    let tapland = t.card("", "Land", None, "This land enters tapped.\n{T}: Add {U}.");
    let mut g = Game::new(t);
    g.put(moon, P1, Zone::Battlefield);
    let land = g.put(tapland, P0, Zone::Hand);
    g.main();
    g.act(Action::PlayLand { object: land }, &[], &[]);
    let id = g.find(tapland).expect("played");
    assert!(!g.engine.state.objects[&id].tapped);
}

#[test]
fn without_blood_moon_it_enters_tapped() {
    let mut t = Table::default();
    let tapland = t.card("", "Land", None, "This land enters tapped.\n{T}: Add {U}.");
    let mut g = Game::new(t);
    let land = g.put(tapland, P0, Zone::Hand);
    g.main();
    g.act(Action::PlayLand { object: land }, &[], &[]);
    let id = g.find(tapland).expect("played");
    assert!(g.engine.state.objects[&id].tapped);
}
