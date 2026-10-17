//! "You may activate abilities of creatures you control as though those creatures had
//! haste." (Thousand-Year Elixir)

use super::harness::*;
use mtg_core::Zone;

fn sick_creature_can_tap(elixir: bool) -> bool {
    let mut t = Table::default();
    let flask = t.card(
        "{3}",
        "Artifact",
        None,
        "You may activate abilities of creatures you control as though those creatures had haste.",
    );
    let looter = t.card(
        "{0}",
        "Creature — Human Wizard",
        Some((1, 1)),
        "{T}: Draw a card.",
    );
    let mut g = Game::new(t);
    if elixir {
        g.put(flask, P0, Zone::Battlefield);
    }
    let s = g.put(looter, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    let creature = g.find(looter).expect("cast");
    offers(&g.main(), creature)
}

#[test]
fn a_summoning_sick_creature_can_tap_with_the_elixir() {
    assert!(!sick_creature_can_tap(false));
    assert!(sick_creature_can_tap(true));
}
