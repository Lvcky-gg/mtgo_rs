//! "Whenever enchanted land is tapped for mana, its controller adds an additional {G}."
//! (Wild Growth, Utopia Sprawl) and "Whenever you tap a creature for mana, add an
//! additional {G}." (Badgermole Cub): more mana as the permanent is tapped (CR 605.1b).

use super::harness::*;
use mtg_core::{Color, Zone};
use mtg_engine::actions::Action;

const GROWTH: &str = "Enchant land\nWhenever enchanted land is tapped for mana, its controller \
                      adds an additional {G}.";
const SPRAWL: &str = "Enchant Forest\nAs this Aura enters, choose a color.\nWhenever enchanted \
                      Forest is tapped for mana, its controller adds an additional one mana of \
                      the chosen color.";

/// One land enchanted by `aura` (with `chosen` as its chosen color): can P0 cast `cost`?
fn enchanted_land(aura: &str, chosen: Option<Color>, cost: &str) -> bool {
    let mut table = Table::default();
    let land = table.card("", "Basic Land — Forest", None, "{T}: Add {G}.");
    let aura = table.card("{G}", "Enchantment — Aura", None, aura);
    let spell = table.card(cost, "Instant", None, "You gain 1 life.");
    let mut game = Game::new(table);
    let land = game.put(land, P0, Zone::Battlefield);
    let aura = game.put(aura, P0, Zone::Battlefield);
    let a = game.engine.state.objects.get_mut(&aura).unwrap();
    a.attached_to = Some(land);
    a.chosen_color = chosen;
    let spell = game.put(spell, P0, Zone::Hand);
    let can = game.main().contains(&Action::Cast { object: spell });
    if can {
        game.cast(spell, &[]);
        assert_eq!(game.life(P0), 21);
    }
    can
}

#[test]
fn wild_growth_adds_a_green() {
    assert!(enchanted_land(GROWTH, None, "{G}{G}"));
    assert!(!enchanted_land("Enchant land", None, "{G}{G}"));
}

#[test]
fn utopia_sprawl_adds_the_chosen_color() {
    assert!(enchanted_land(SPRAWL, Some(Color::Blue), "{G}{U}"));
    assert!(!enchanted_land(SPRAWL, Some(Color::Blue), "{G}{R}"));
}

#[test]
fn badgermole_cub_adds_a_green_for_a_creature_tapped_for_mana() {
    let mut table = Table::default();
    let cub = table.card(
        "{1}{G}",
        "Creature — Badger Mole",
        Some((2, 2)),
        "Whenever you tap a creature for mana, add an additional {G}.",
    );
    let elf = table.card("{G}", "Creature — Elf Druid", Some((1, 1)), "{T}: Add {G}.");
    let spell = table.card("{G}{G}", "Instant", None, "You gain 1 life.");
    let mut game = Game::new(table);
    game.put(cub, P0, Zone::Battlefield);
    game.put(elf, P0, Zone::Battlefield);
    let spell = game.put(spell, P0, Zone::Hand);
    assert!(game.main().contains(&Action::Cast { object: spell }));
    game.cast(spell, &[]);
    assert_eq!(game.life(P0), 21);
}

/// One permanent taps once: two mana abilities that both tap it can't both pay.
#[test]
fn a_land_with_two_tap_abilities_makes_one_mana() {
    let mut table = Table::default();
    let land = table.card("", "Land", None, "{T}: Add {G}.\n{T}: Add {R}.");
    let spell = table.card("{G}{R}", "Instant", None, "You gain 1 life.");
    let mut game = Game::new(table);
    game.put(land, P0, Zone::Battlefield);
    let spell = game.put(spell, P0, Zone::Hand);
    assert!(!game.main().contains(&Action::Cast { object: spell }));
}

/// The merged permanent activates the ability that makes the color spent: a painland
/// paying {R} uses its damaging ability, not the {C} one.
#[test]
fn a_painland_pays_red_with_its_painful_ability() {
    let mut table = Table::default();
    let land = table.card(
        "",
        "Land",
        None,
        "{T}: Add {C}.\n{T}: Add {R} or {G}. This land deals 1 damage to you.",
    );
    let spell = table.card("{R}", "Instant", None, "You gain 3 life.");
    let mut game = Game::new(table);
    game.put(land, P0, Zone::Battlefield);
    let spell = game.put(spell, P0, Zone::Hand);
    assert!(game.main().contains(&Action::Cast { object: spell }));
    game.cast(spell, &[]);
    assert_eq!(game.life(P0), 22, "3 gained, 1 from the land");
}
