//! "they" in a trigger's effect: the player the event happened to.

use super::harness::*;
use mtg_core::{Step, Zone};

#[test]
fn sheoldred_drains_an_opponent_who_draws_and_feeds_you_when_you_draw() {
    let mut t = Table::default();
    let sheoldred = t.card(
        "{2}{B}{B}",
        "Legendary Creature — Phyrexian Praetor",
        Some((4, 5)),
        "Deathtouch\nWhenever you draw a card, you gain 2 life.\nWhenever an opponent draws a card, they lose 2 life.",
    );
    let mut g = Game::new(t);
    g.put(sheoldred, P0, Zone::Battlefield);
    // Your draw step: you gain 2.
    g.main();
    assert_eq!((g.life(P0), g.life(P1)), (22, 20));
    // Their draw step: they lose 2.
    g.pending = None;
    g.until(P1, Step::PrecombatMain);
    assert_eq!((g.life(P0), g.life(P1)), (22, 18));
}
