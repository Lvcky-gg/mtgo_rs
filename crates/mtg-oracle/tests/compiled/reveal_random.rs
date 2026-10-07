//! "Target opponent reveals a card at random from their hand. … that card's mana value."

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

/// P1 holds one card, a two-mana Bear, so the random card is known.
fn with_one_card_in_p1s_hand(text: &str, mana: usize) -> (Game, mtg_core::ObjectId) {
    let mut table = Table::default();
    let spell = table.card("{1}{R}", "Sorcery", None, text);
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(mana);
    let spell = game.put(spell, P0, Zone::Hand);
    game.put(bear, P1, Zone::Hand);
    game.main();
    (game, spell)
}

#[test]
fn damage_equal_to_the_revealed_cards_mana_value() {
    let (mut game, spell) = with_one_card_in_p1s_hand(
        "Target player reveals a card at random from their hand. ~ deals damage to that \
         player equal to that card's mana value.",
        2,
    );
    game.act(Action::Cast { object: spell }, &[Target::Player(P1)], &[]);
    assert_eq!(game.life(P1), 18);
    assert_eq!(game.count(Zone::Hand, P1), 1, "revealed, not discarded");
}

#[test]
fn x_is_the_revealed_cards_mana_value() {
    let mut table = Table::default();
    let favor = table.card(
        "{2}{G}",
        "Enchantment",
        None,
        "{3}{G}: Target opponent reveals a card at random from their hand. Target creature gets \
         +X/+X until end of turn, where X is the revealed card's mana value.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(4);
    let favor = game.put(favor, P0, Zone::Battlefield);
    let mine = game.put(bear, P0, Zone::Battlefield);
    game.put(bear, P1, Zone::Hand);
    game.main();
    game.act(
        activate(favor, 0),
        &[Target::Player(P1), Target::Object(mine)],
        &[],
    );
    assert_eq!(game.pt(mine), (4, 4));
}
