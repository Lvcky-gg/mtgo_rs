//! Command Tower and Arcane Signet: "{T}: Add one mana of any color in your commander's
//! color identity." (CR 903.4) — the colors of the commander's mana symbols, in its cost
//! and its rules text; nothing without a commander (CR 903.4f).

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

const TOWER: &str = "{T}: Add one mana of any color in your commander's color identity.";

/// One spell per color in hand, a Command Tower out, and `commander` (cost, text) as P0's
/// commander. Returns which of W, U, B, R, G spells P0 can cast.
fn castable(commander: Option<(&str, &str)>) -> [bool; 5] {
    let mut table = Table::default();
    let tower = table.card("", "Land", None, TOWER);
    let spells: Vec<_> = ["{W}", "{U}", "{B}", "{R}", "{G}"]
        .into_iter()
        .map(|cost| table.card(cost, "Instant", None, "You gain 1 life."))
        .collect();
    let general = commander
        .map(|(cost, text)| table.card(cost, "Legendary Creature — Elf Druid", Some((2, 2)), text));
    let mut game = Game::new(table);
    if let Some(general) = general {
        game.engine.state.commander.commanders.insert(P0, general);
        game.put(general, P0, Zone::Command);
    }
    game.put(tower, P0, Zone::Battlefield);
    let spells: Vec<_> = spells
        .into_iter()
        .map(|s| game.put(s, P0, Zone::Hand))
        .collect();
    let actions = game.main();
    let mut out = [false; 5];
    for (i, s) in spells.iter().enumerate() {
        out[i] = actions.contains(&Action::Cast { object: *s });
    }
    out
}

#[test]
fn the_commanders_cost_decides_the_colors() {
    assert_eq!(
        castable(Some(("{1}{G}{U}", ""))),
        [false, true, false, false, true]
    );
}

#[test]
fn hybrid_and_rules_text_symbols_count() {
    assert_eq!(
        castable(Some(("{R/W}", "{T}: Add {B}."))),
        [true, false, true, true, false]
    );
}

#[test]
fn no_commander_no_mana() {
    assert_eq!(castable(None), [false; 5]);
}

#[test]
fn a_colorless_commander_makes_no_mana() {
    assert_eq!(castable(Some(("{4}", ""))), [false; 5]);
}

#[test]
fn the_tower_pays_for_a_spell() {
    let mut table = Table::default();
    let tower = table.card("", "Land", None, TOWER);
    let spell = table.card("{G}", "Instant", None, "You gain 3 life.");
    let general = table.card("{G}", "Legendary Creature — Elf", Some((1, 1)), "");
    let mut game = Game::new(table);
    game.engine.state.commander.commanders.insert(P0, general);
    game.put(general, P0, Zone::Command);
    let tower = game.put(tower, P0, Zone::Battlefield);
    let spell = game.put(spell, P0, Zone::Hand);
    game.main();
    game.cast(spell, &[]);
    assert_eq!(game.life(P0), 23);
    assert!(game.engine.state.objects[&tower].tapped);
}
