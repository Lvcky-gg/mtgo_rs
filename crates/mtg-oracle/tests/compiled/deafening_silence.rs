//! Deafening Silence: "Each player can't cast more than one noncreature spell each turn."

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

#[test]
fn one_noncreature_spell_then_only_creatures() {
    let mut table = Table::default();
    let silence = table.card(
        "{W}",
        "Enchantment",
        None,
        "Each player can't cast more than one noncreature spell each turn.",
    );
    let sorcery = table.card("{0}", "Sorcery", None, "You gain 1 life.");
    let bear = table.card("{0}", "Creature — Bear", Some((2, 2)), "");
    let mut game = Game::new(table);
    game.put(silence, P0, Zone::Battlefield);
    let first = game.put(sorcery, P0, Zone::Hand);
    let second = game.put(sorcery, P0, Zone::Hand);
    let bear = game.put(bear, P0, Zone::Hand);
    let actions = game.main();
    assert!(actions.contains(&Action::Cast { object: first }));
    game.act(Action::Cast { object: bear }, &[], &[]);
    let actions = game.until(P0, mtg_core::Step::PrecombatMain);
    assert!(
        actions.contains(&Action::Cast { object: first }),
        "a creature spell doesn't count"
    );
    game.cast(first, &[]);
    let actions = game.until(P0, mtg_core::Step::PrecombatMain);
    assert!(!actions.contains(&Action::Cast { object: second }));
}
