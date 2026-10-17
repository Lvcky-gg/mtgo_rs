//! "Choose up to one —" (Hullbreaker Horror): a mode, or none at all.

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};

const HORROR: &str = "Whenever you cast a spell, choose up to one —\n• Return target spell you don't control to its owner's hand.\n• Return target nonland permanent to its owner's hand.";

fn setup() -> (Game, mtg_core::ObjectId, mtg_core::ObjectId) {
    let mut t = Table::default();
    let horror = t.card(
        "{5}{U}{U}",
        "Creature — Kraken Horror",
        Some((7, 8)),
        HORROR,
    );
    let trinket = t.card("{0}", "Instant", None, "You gain 1 life.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(horror, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(trinket, P0, Zone::Hand);
    g.main();
    (g, spell, theirs)
}

#[test]
fn choosing_no_mode_does_nothing() {
    let (mut g, spell, theirs) = setup();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Modes(vec![])],
    );
    assert!(g.engine.state.objects.contains_key(&theirs));
    assert_eq!(g.life(P0), 21);
}

#[test]
fn choosing_the_second_mode_bounces_a_permanent() {
    let (mut g, spell, theirs) = setup();
    g.act(
        Action::Cast { object: spell },
        &[Target::Object(theirs)],
        &[Answer::Modes(vec![1])],
    );
    assert!(!g.engine.state.objects.contains_key(&theirs));
    assert_eq!(g.count(Zone::Hand, P1), 1);
}
