//! Tymna the Weaver: "At the beginning of each of your postcombat main phases, you may pay X
//! life, where X is the number of opponents that were dealt combat damage this turn. If you
//! do, draw X cards."

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::choice::Answer;

const TYMNA: &str = "Lifelink\nAt the beginning of each of your postcombat main phases, you \
                     may pay X life, where X is the number of opponents that were dealt combat \
                     damage this turn. If you do, draw X cards.";

#[test]
fn draws_for_each_opponent_hit_in_combat() {
    let mut t = Table::default();
    let tymna = t.card(
        "{1}{W}{B}",
        "Legendary Creature — Human Cleric",
        Some((2, 2)),
        TYMNA,
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(tymna, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.combat(&[b], &[], &[], &[Answer::Bool(true)]);
    assert_eq!(g.life(P1), 18);
    assert_eq!(g.life(P0), 19, "paid 1 life");
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
}

#[test]
fn noncombat_damage_does_not_count() {
    let mut t = Table::default();
    let tymna = t.card(
        "{1}{W}{B}",
        "Legendary Creature — Human Cleric",
        Some((2, 2)),
        TYMNA,
    );
    let shock = t.card(
        "{0}",
        "Instant",
        None,
        "This spell deals 2 damage to any target.",
    );
    let mut g = Game::new(t);
    g.put(tymna, P0, Zone::Battlefield);
    let shock = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(shock, &[Target::Player(P1)]);
    let hand = g.count(Zone::Hand, P0);
    g.combat(&[], &[], &[], &[Answer::Bool(true)]);
    assert_eq!(g.life(P0), 20);
    assert_eq!(g.count(Zone::Hand, P0), hand);
}
