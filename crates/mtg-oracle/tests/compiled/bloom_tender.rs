//! Bloom Tender: "Vivid — {T}: For each color among permanents you control, add one mana
//! of that color."

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

fn can_cast_gu(with_blue: bool) -> bool {
    let mut table = Table::default();
    let tender = table.card(
        "{1}{G}",
        "Creature — Elf Druid",
        Some((1, 1)),
        "Vivid — {T}: For each color among permanents you control, add one mana of that color.",
    );
    let blue = table.card("{U}", "Enchantment", None, "");
    let spell = table.card("{G}{U}", "Instant", None, "You gain 1 life.");
    let mut game = Game::new(table);
    game.put(tender, P0, Zone::Battlefield);
    if with_blue {
        game.put(blue, P0, Zone::Battlefield);
    }
    let spell = game.put(spell, P0, Zone::Hand);
    let actions = game.main();
    let can = actions.contains(&Action::Cast { object: spell });
    if can {
        game.cast(spell, &[]);
        assert_eq!(game.life(P0), 21);
    }
    can
}

#[test]
fn one_mana_of_each_color_among_your_permanents() {
    assert!(can_cast_gu(true));
}

#[test]
fn only_the_colors_there_are() {
    assert!(!can_cast_gu(false));
}
