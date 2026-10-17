//! An amount naming "that player" / "their" for a group of players is read for each of them:
//! "~ deals damage to each player equal to twice the number of nonbasic lands that player
//! controls" (Price of Progress). "Your opponents control" still counts them all.

use super::harness::*;
use mtg_core::Zone;

#[test]
fn price_of_progress_counts_each_players_own_lands() {
    let mut t = Table::default();
    let price = t.card(
        "{0}",
        "Instant",
        None,
        "~ deals damage to each player equal to twice the number of nonbasic lands that player controls.",
    );
    let dual = t.card("", "Land", None, "{T}: Add {U} or {B}.");
    let mut g = Game::new(t);
    let spell = g.put(price, P0, Zone::Hand);
    g.put(dual, P0, Zone::Battlefield);
    for _ in 0..3 {
        g.put(dual, P1, Zone::Battlefield);
    }
    g.main();
    g.cast(spell, &[]);
    assert_eq!(g.life(P0), 18, "one nonbasic: 2");
    assert_eq!(g.life(P1), 14, "three nonbasics: 6");
}

#[test]
fn each_player_draws_for_their_own_hand() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Each player draws cards equal to the number of cards in their hand.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let spell = g.put(spell, P0, Zone::Hand);
    g.put(bear, P1, Zone::Hand);
    g.put(bear, P1, Zone::Hand);
    g.main();
    // P0: the turn's draw only, once the spell has left; P1: two.
    g.cast(spell, &[]);
    assert_eq!(g.count(Zone::Hand, P0), 2);
    assert_eq!(g.count(Zone::Hand, P1), 4);
}

#[test]
fn your_opponents_still_means_all_of_them() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Each opponent loses life equal to the number of creatures your opponents control.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let spell = g.put(spell, P0, Zone::Hand);
    g.put(bear, P1, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.cast(spell, &[]);
    assert_eq!(g.life(P1), 18);
}

#[test]
fn each_player_loses_half_their_own_life() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Each player loses half their life, rounded up.",
    );
    let mut g = Game::new(t);
    let spell = g.put(spell, P0, Zone::Hand);
    g.engine.state.players.get_mut(&P1).unwrap().life = 7;
    g.main();
    g.cast(spell, &[]);
    assert_eq!(g.life(P0), 10);
    assert_eq!(g.life(P1), 3, "half of 7, rounded up, is 4");
}
