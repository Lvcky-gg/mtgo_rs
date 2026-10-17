//! CR 402.2 / 514.1 — maximum hand size: "you have no maximum hand size", "each opponent's
//! maximum hand size is reduced by two", "your maximum hand size is equal to …", and a
//! spell's "you have no maximum hand size for the rest of the game".

use super::harness::*;
use mtg_core::{Step, Zone};

fn hand(g: &mut Game, who: mtg_core::PlayerId, n: usize) {
    let filler = g.table.bear();
    for _ in 0..n {
        g.put(filler, who, Zone::Hand);
    }
}

#[test]
fn seven_is_the_default() {
    let mut g = Game::new(Table::default());
    hand(&mut g, P0, 9);
    g.until(P1, Step::Upkeep);
    assert_eq!(g.count(Zone::Hand, P0), 7);
}

#[test]
fn no_maximum_hand_size_keeps_every_card() {
    let mut t = Table::default();
    let spire = t.card("{3}", "Artifact", None, "You have no maximum hand size.");
    let mut g = Game::new(t);
    g.put(spire, P0, Zone::Battlefield);
    hand(&mut g, P0, 9);
    g.until(P1, Step::Upkeep);
    assert_eq!(g.count(Zone::Hand, P0), 10, "nine and the turn's draw");
}

#[test]
fn an_opponents_maximum_is_reduced() {
    let mut t = Table::default();
    let gag = t.card(
        "{2}",
        "Artifact",
        None,
        "Each opponent's maximum hand size is reduced by two.",
    );
    let mut g = Game::new(t);
    g.put(gag, P0, Zone::Battlefield);
    hand(&mut g, P0, 7);
    hand(&mut g, P1, 7);
    g.until(P1, Step::Upkeep);
    assert_eq!(g.count(Zone::Hand, P0), 7, "the controller is unaffected");
    // P1 draws to eight and keeps five.
    g.until(P0, Step::Upkeep);
    assert_eq!(g.count(Zone::Hand, P1), 5);
}

#[test]
fn equal_to_sets_it_and_increases_add() {
    let mut t = Table::default();
    let set = t.card(
        "{2}",
        "Artifact",
        None,
        "Your maximum hand size is equal to the number of artifacts you control.",
    );
    let plus = t.card(
        "{2}",
        "Artifact",
        None,
        "Your maximum hand size is increased by one.",
    );
    let mut g = Game::new(t);
    g.put(set, P0, Zone::Battlefield);
    g.put(plus, P0, Zone::Battlefield);
    hand(&mut g, P0, 9);
    g.until(P1, Step::Upkeep);
    assert_eq!(g.count(Zone::Hand, P0), 3, "two artifacts, plus one");
}

#[test]
fn for_the_rest_of_the_game_outlasts_the_spell() {
    let mut t = Table::default();
    let restoration = t.card(
        "{1}",
        "Sorcery",
        None,
        "Draw a card. You have no maximum hand size for the rest of the game.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(restoration, P0, Zone::Hand);
    hand(&mut g, P0, 9);
    g.main();
    g.cast(spell, &[]);
    // Nine, the turn's draw and the spell's.
    assert_eq!(g.count(Zone::Hand, P0), 11);
    g.until(P1, Step::Upkeep);
    assert_eq!(g.count(Zone::Hand, P0), 11);
    // Still none on a later turn.
    g.until(P0, Step::Upkeep);
    g.until(P1, Step::Upkeep);
    assert_eq!(g.count(Zone::Hand, P0), 12);
}
