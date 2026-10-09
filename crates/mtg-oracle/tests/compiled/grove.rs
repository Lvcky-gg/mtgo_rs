//! Grove of the Burnwillows: "{T}: Add {R} or {G}. Each opponent gains 1 life." — a mana
//! ability whose follow-up resolves with it, also when tapped to pay for a spell.

use super::harness::*;
use mtg_core::Zone;

#[test]
fn tapping_it_for_a_spell_gives_each_opponent_a_life() {
    let mut table = Table::default();
    let grove = table.card(
        "",
        "Land",
        None,
        "{T}: Add {R} or {G}. Each opponent gains 1 life.",
    );
    let spell = table.card("{G}", "Instant", None, "You gain 3 life.");
    let mut game = Game::new(table);
    game.put(grove, P0, Zone::Battlefield);
    let spell = game.put(spell, P0, Zone::Hand);
    game.main();
    game.cast(spell, &[]);
    assert_eq!(game.life(P0), 23);
    assert_eq!(game.life(P1), 21);
}
