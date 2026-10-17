//! "… return that card to the battlefield … with a +1/+1 counter on it" (Daydream), and
//! "has lifelink and ward {2}".

use super::harness::*;
use mtg_core::{CounterKind, Keyword, Target, Zone};

#[test]
fn daydream_blinks_with_a_counter() {
    let mut t = Table::default();
    let daydream = t.card(
        "{W}",
        "Sorcery",
        None,
        "Exile target creature you control, then return that card to the battlefield under its owner's control with a +1/+1 counter on it.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(daydream, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(b)]);
    let back = g.find(bear).expect("returned");
    assert_ne!(back, b, "a new object");
    assert_eq!(
        g.engine.state.objects[&back]
            .counters
            .get(&CounterKind::PlusOnePlusOne),
        Some(&1)
    );
    assert_eq!(g.pt(back), (3, 3));
}

#[test]
fn a_static_grants_lifelink_and_ward() {
    let mut t = Table::default();
    let banner = t.card(
        "{2}",
        "Enchantment",
        None,
        "Creatures you control have lifelink and ward {2}.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(banner, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    assert!(g.has(b, Keyword::Lifelink));
}
