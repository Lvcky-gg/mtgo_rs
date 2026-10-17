//! "You may play two additional lands on each of your turns." (Azusa), "Lands you control
//! are every basic land type in addition to their other types." (Dryad of the Ilysian
//! Grove), "Target creature becomes blue until end of turn." (Cerulean Wisps), and "Elves"
//! read as the plural of Elf even though a card prints "Elves" as a subtype.

use super::harness::*;
use mtg_core::{Color, Target, Zone};
use mtg_engine::actions::Action;
use mtg_ir::PrintedCards;

fn plays(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|a| matches!(a, Action::PlayLand { .. }))
        .count()
}

#[test]
fn azusa_allows_three_land_plays() {
    let mut t = Table::default();
    let azusa = t.card(
        "{2}{G}",
        "Legendary Creature — Human Monk",
        Some((0, 2)),
        "You may play two additional lands on each of your turns.",
    );
    let forests: Vec<_> = (0..4)
        .map(|_| t.card("", "Basic Land — Forest", None, ""))
        .collect();
    let mut g = Game::new(t);
    g.put(azusa, P0, Zone::Battlefield);
    let lands: Vec<_> = forests.iter().map(|&f| g.put(f, P0, Zone::Hand)).collect();
    g.main();
    for land in &lands[..3] {
        g.act(Action::PlayLand { object: *land }, &[], &[]);
    }
    assert_eq!(
        g.count(Zone::Hand, P0),
        2,
        "three played, the drawn card and one Forest left"
    );
    assert_eq!(plays(&g.main()), 0, "no fourth play");
}

#[test]
fn three_additional_lands_this_turn() {
    let mut t = Table::default();
    let bloom = t.card(
        "{1}{G}",
        "Sorcery",
        None,
        "You may play three additional lands this turn.",
    );
    let forests: Vec<_> = (0..5)
        .map(|_| t.card("", "Basic Land — Forest", None, ""))
        .collect();
    let mut g = Game::new(t);
    g.lands(2);
    let bloom = g.put(bloom, P0, Zone::Hand);
    let lands: Vec<_> = forests.iter().map(|&f| g.put(f, P0, Zone::Hand)).collect();
    g.main();
    g.cast(bloom, &[]);
    for land in &lands[..4] {
        assert!(plays(&g.main()) > 0);
        g.act(Action::PlayLand { object: *land }, &[], &[]);
    }
    assert_eq!(plays(&g.main()), 0, "four plays in all");
}

#[test]
fn every_basic_land_type_lets_a_forest_tap_for_blue() {
    let mut t = Table::default();
    let dryad = t.card(
        "{1}{G}",
        "Enchantment Creature — Nymph Dryad",
        Some((2, 4)),
        "Lands you control are every basic land type in addition to their other types.",
    );
    let forest = t.card("", "Basic Land — Forest", None, "");
    let blue = t.card("{U}", "Creature — Bird", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.put(dryad, P0, Zone::Battlefield);
    let land = g.put(forest, P0, Zone::Battlefield);
    let blue = g.put(blue, P0, Zone::Hand);
    assert!(g.main().contains(&Action::Cast { object: blue }));
    let ch = g.engine.characteristics(&g.table, land).unwrap();
    for name in ["Plains", "Island", "Swamp", "Mountain", "Forest"] {
        let st = g.table.subtype_named(name).unwrap();
        assert!(ch.subtypes.contains(&st), "{name}");
    }
}

#[test]
fn an_opponents_land_is_untouched() {
    let mut t = Table::default();
    let dryad = t.card(
        "{1}{G}",
        "Enchantment Creature — Nymph Dryad",
        Some((2, 4)),
        "Lands you control are every basic land type in addition to their other types.",
    );
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    g.put(dryad, P0, Zone::Battlefield);
    let land = g.put(forest, P1, Zone::Battlefield);
    g.main();
    let ch = g.engine.characteristics(&g.table, land).unwrap();
    let island = g.table.subtype_named("Island").unwrap();
    assert!(!ch.subtypes.contains(&island));
}

#[test]
fn becomes_blue_replaces_its_colors_until_end_of_turn() {
    let mut t = Table::default();
    let wisps = t.card(
        "{U}",
        "Instant",
        None,
        "Target creature becomes blue until end of turn. Untap that creature.",
    );
    let goblin = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let wisps = g.put(wisps, P0, Zone::Hand);
    let target = g.put(goblin, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&target).unwrap().tapped = true;
    g.main();
    g.cast(wisps, &[Target::Object(target)]);
    let ch = g.engine.characteristics(&g.table, target).unwrap();
    assert!(ch.colors.contains(Color::Blue));
    assert!(!ch.colors.contains(Color::Red));
    assert!(!g.engine.state.objects[&target].tapped);
    g.until(P1, mtg_core::Step::Upkeep);
    let ch = g.engine.characteristics(&g.table, target).unwrap();
    assert!(ch.colors.contains(Color::Red));
}

#[test]
fn elves_means_every_elf() {
    let mut t = Table::default();
    let lord = t.card(
        "{1}{G}",
        "Enchantment",
        None,
        "Elves you control get +1/+1.",
    );
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.put(lord, P0, Zone::Battlefield);
    let elf = g.put(elf, P0, Zone::Battlefield);
    g.main();
    assert_eq!(g.pt(elf), (2, 2));
}
