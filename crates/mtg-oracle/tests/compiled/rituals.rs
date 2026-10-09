//! Cabal Ritual ("Threshold — Add {B}{B}{B}{B}{B} instead if there are seven or more
//! cards in your graveyard.") and Rite of Flame ("Add {R}{R}, then add {R} for each card
//! named ~ in each graveyard.").

use super::harness::*;
use mtg_core::{Color, Zone};

const CABAL: &str = "Add {B}{B}{B}.\nThreshold — Add {B}{B}{B}{B}{B} instead if there are \
                     seven or more cards in your graveyard.";
const RITE: &str = "Add {R}{R}, then add {R} for each card named ~ in each graveyard.";

fn pool(game: &Game, color: Color) -> u16 {
    game.engine.state.player(P0).mana.amounts[color as usize]
}

fn cabal(graveyard: usize) -> u16 {
    let mut table = Table::default();
    let ritual = table.card("{0}", "Instant", None, CABAL);
    let filler = table.bear();
    let mut game = Game::new(table);
    for _ in 0..graveyard {
        game.put(filler, P0, Zone::Graveyard);
    }
    let ritual = game.put(ritual, P0, Zone::Hand);
    game.main();
    game.cast(ritual, &[]);
    pool(&game, Color::Black)
}

#[test]
fn cabal_ritual_adds_three_without_threshold() {
    assert_eq!(cabal(6), 3);
}

#[test]
fn cabal_ritual_adds_five_with_threshold() {
    assert_eq!(cabal(7), 5);
}

#[test]
fn rite_of_flame_counts_copies_in_every_graveyard() {
    let mut table = Table::default();
    let rite = table.named_card("Rite of Flame", "{0}", "Sorcery", None, RITE);
    let other = table.card("{0}", "Sorcery", None, "You gain 1 life.");
    let mut game = Game::new(table);
    game.put(rite, P0, Zone::Graveyard);
    game.put(rite, P1, Zone::Graveyard);
    game.put(other, P1, Zone::Graveyard);
    let cast = game.put(rite, P0, Zone::Hand);
    game.main();
    game.cast(cast, &[]);
    assert_eq!(
        pool(&game, Color::Red),
        4,
        "{{R}}{{R}} plus one for each of two copies"
    );
}

#[test]
fn rite_of_flame_alone_adds_two() {
    let mut table = Table::default();
    let rite = table.named_card("Rite of Flame", "{0}", "Sorcery", None, RITE);
    let mut game = Game::new(table);
    let cast = game.put(rite, P0, Zone::Hand);
    game.main();
    game.cast(cast, &[]);
    assert_eq!(pool(&game, Color::Red), 2);
}
