//! "an opponent controls more lands than you" (Land Tax), "you have fewer cards in hand
//! than an opponent", "an opponent has more life than you".

use super::harness::*;
use mtg_core::Zone;

fn pt_with(cond: &str, setup: impl Fn(&mut Game, mtg_core::CardId)) -> (i32, i32) {
    let mut t = Table::default();
    let c = t.card(
        "{0}",
        "Creature — Bear",
        Some((2, 2)),
        &format!("As long as {cond}, this creature gets +1/+1."),
    );
    let land = t.mountain();
    let mut g = Game::new(t);
    let id = g.put(c, P0, Zone::Battlefield);
    setup(&mut g, land);
    g.main();
    g.pt(id)
}

#[test]
fn an_opponent_controls_more_lands_than_you() {
    let cond = "an opponent controls more lands than you";
    let even = pt_with(cond, |g, land| {
        g.put(land, P0, Zone::Battlefield);
        g.put(land, P1, Zone::Battlefield);
    });
    assert_eq!(even, (2, 2), "tied is not more");
    let ahead = pt_with(cond, |g, land| {
        g.put(land, P1, Zone::Battlefield);
        g.put(land, P1, Zone::Battlefield);
        g.put(land, P0, Zone::Battlefield);
    });
    assert_eq!(ahead, (3, 3));
}

#[test]
fn fewer_cards_in_hand_and_more_life() {
    let fewer = "you have fewer cards in hand than an opponent";
    let hands = |g: &Game| (g.count(Zone::Hand, P0), g.count(Zone::Hand, P1));
    let more = pt_with(fewer, |g, land| {
        let (a, b) = hands(g);
        for _ in 0..(a + 3).saturating_sub(b) {
            g.put(land, P1, Zone::Hand);
        }
    });
    assert_eq!(more, (3, 3));
    let life = "an opponent has more life than you";
    assert_eq!(pt_with(life, |_, _| {}), (2, 2), "both at 20");
    let lower = pt_with(life, |g, _| {
        g.engine.state.players.get_mut(&P0).unwrap().life = 15;
    });
    assert_eq!(lower, (3, 3));
}
