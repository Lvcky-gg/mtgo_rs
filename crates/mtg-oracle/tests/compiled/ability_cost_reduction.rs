//! "Activated abilities of creatures you control cost {2} less to activate. This effect
//! can't reduce the mana in that cost to less than one mana." (Training Grounds)

use super::harness::*;
use mtg_core::Zone;

const GROUNDS: &str = "Activated abilities of creatures you control cost {2} less to activate. This effect can't reduce the mana in that cost to less than one mana.";

fn can_activate(text: &str, lands: usize, grounds: bool) -> bool {
    let mut t = Table::default();
    let wizard = t.card("{U}", "Creature — Wizard", Some((1, 1)), text);
    let enchantment = t.card("{U}", "Enchantment", None, GROUNDS);
    let mut g = Game::new(t);
    g.lands(lands);
    let source = g.put(wizard, P0, Zone::Battlefield);
    if grounds {
        g.put(enchantment, P0, Zone::Battlefield);
    }
    offers(&g.main(), source)
}

#[test]
fn generic_mana_comes_off() {
    assert!(!can_activate("{4}: Draw a card.", 2, false));
    assert!(can_activate("{4}: Draw a card.", 2, true));
    assert!(!can_activate("{4}: Draw a card.", 1, true));
}

#[test]
fn at_least_one_mana_remains() {
    // {3}: reduced to {1}, never to nothing.
    assert!(can_activate("{3}: Draw a card.", 1, true));
    assert!(!can_activate("{3}: Draw a card.", 0, true));
    // {1}{U}: only generic mana comes off, so the {1} goes and {U} remains.
    assert!(can_activate("{1}{U}: Draw a card.", 1, true));
    assert!(!can_activate("{1}{U}: Draw a card.", 0, true));
}

#[test]
fn the_reduction_is_actually_paid_that_way() {
    let mut t = Table::default();
    let wizard = t.card(
        "{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "{4}: Draw a card.",
    );
    let enchantment = t.card("{U}", "Enchantment", None, GROUNDS);
    let mut g = Game::new(t);
    g.lands(2);
    let source = g.put(wizard, P0, Zone::Battlefield);
    g.put(enchantment, P0, Zone::Battlefield);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(activate(source, 0), &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
    let tapped = g
        .engine
        .state
        .battlefield()
        .iter()
        .filter(|id| g.engine.state.objects[id].tapped)
        .count();
    assert_eq!(tapped, 2);
}
