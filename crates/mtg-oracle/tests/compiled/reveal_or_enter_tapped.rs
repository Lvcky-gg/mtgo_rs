//! "As this land enters, you may reveal a Mountain or Plains card from your hand. If you
//! don't, this land enters tapped." (Furycalm Snarl)

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{actions::Action, choice::Answer};

const SNARL: &str = "As this land enters, you may reveal a Mountain or Plains card from your hand. If you don't, this land enters tapped.\n{T}: Add {R} or {W}.";

fn tapped(g: &Game, card: mtg_core::CardId) -> bool {
    let id = g.find(card).expect("on the battlefield");
    g.engine.state.objects[&id].tapped
}

#[test]
fn revealing_a_matching_card_keeps_it_untapped() {
    let mut t = Table::default();
    let snarl = t.card("", "Land", None, SNARL);
    let mountain = t.mountain();
    let mut g = Game::new(t);
    let land = g.put(snarl, P0, Zone::Hand);
    g.put(mountain, P0, Zone::Hand);
    assert!(g.main().contains(&Action::PlayLand { object: land }));
    g.act(Action::PlayLand { object: land }, &[], &[]);
    assert!(!tapped(&g, snarl));
}

#[test]
fn declining_to_reveal_enters_tapped() {
    let mut t = Table::default();
    let snarl = t.card("", "Land", None, SNARL);
    let mountain = t.mountain();
    let mut g = Game::new(t);
    let land = g.put(snarl, P0, Zone::Hand);
    g.put(mountain, P0, Zone::Hand);
    g.main();
    g.act(
        Action::PlayLand { object: land },
        &[],
        &[Answer::Objects(vec![])],
    );
    assert!(tapped(&g, snarl));
}

#[test]
fn without_a_matching_card_it_enters_tapped() {
    let mut t = Table::default();
    let snarl = t.card("", "Land", None, SNARL);
    let island = t.card("", "Basic Land — Island", None, "{T}: Add {U}.");
    let mut g = Game::new(t);
    let land = g.put(snarl, P0, Zone::Hand);
    g.put(island, P0, Zone::Hand);
    g.main();
    g.act(Action::PlayLand { object: land }, &[], &[]);
    assert!(tapped(&g, snarl));
}
