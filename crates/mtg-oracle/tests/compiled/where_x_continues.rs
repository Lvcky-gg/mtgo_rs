//! A "where X is …" definition followed by a sentence that doesn't use X.

use super::harness::*;
use mtg_core::Zone;

#[test]
fn gray_merchant_drains_by_devotion_and_the_next_sentence_still_happens() {
    let mut t = Table::default();
    let merchant = t.card(
        "{3}{B}{B}",
        "Creature — Zombie",
        Some((2, 4)),
        "When this creature enters, each opponent loses X life, where X is your devotion to black. You gain life equal to the life lost this way.",
    );
    let shrine = t.card("{B}{B}", "Enchantment", None, "");
    let mut g = Game::new(t);
    g.lands(5);
    g.put(shrine, P0, Zone::Battlefield);
    let card = g.put(merchant, P0, Zone::Hand);
    g.main();
    g.cast(card, &[]);
    // Two black symbols on the Merchant, two on the enchantment.
    assert_eq!(g.life(P1), 16);
    assert_eq!(g.life(P0), 24);
}

#[test]
fn a_count_plus_one() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Draw cards equal to the number of cards in your hand plus one.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let s = g.put(spell, P0, Zone::Hand);
    g.put(bear, P0, Zone::Hand);
    g.put(bear, P0, Zone::Hand);
    g.main();
    // The draw step added one: four in hand, then the spell leaves — three, so draw four.
    let before = g.count(Zone::Hand, P0);
    g.cast(s, &[]);
    assert_eq!(g.count(Zone::Hand, P0), (before - 1) * 2 + 1);
}
