//! A static ability listing several things about its subject: "gets +2/+2, has flying, and
//! attacks each combat if able".

use super::harness::*;
use mtg_core::{Keyword, Zone};

const CHANNELER: &str = "Delirium — As long as there are four or more card types among cards in your graveyard, this creature gets +2/+2, has flying, and attacks each combat if able.";

#[test]
fn dragons_rage_channeler_grows_flies_and_must_attack_with_delirium() {
    let mut t = Table::default();
    let channeler = t.card("{R}", "Creature — Human Shaman", Some((1, 1)), CHANNELER);
    let types = [
        t.card("{1}", "Artifact", None, ""),
        t.card("{1}", "Enchantment", None, ""),
        t.card("{1}", "Instant", None, "Draw a card."),
        t.card("", "Land", None, ""),
    ];
    let mut g = Game::new(t);
    let drc = g.put(channeler, P0, Zone::Battlefield);
    assert_eq!(g.pt(drc), (1, 1));
    assert!(!g.has(drc, Keyword::Flying));
    assert!(mtg_engine::combat::must_attack(&g.engine.state, &g.table, P0).is_empty());
    for (i, card) in types.iter().enumerate() {
        g.put(*card, P0, Zone::Graveyard);
        // Three types are not yet delirium.
        if i == 2 {
            assert_eq!(g.pt(drc), (1, 1));
        }
    }
    assert_eq!(g.pt(drc), (3, 3));
    assert!(g.has(drc, Keyword::Flying));
    assert_eq!(
        mtg_engine::combat::must_attack(&g.engine.state, &g.table, P0),
        vec![drc]
    );
}
