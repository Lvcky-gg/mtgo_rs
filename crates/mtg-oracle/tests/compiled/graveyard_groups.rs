//! "Return all creature cards with mana value 2 or less from your graveyard to the
//! battlefield." — a whole group of graveyard cards, only yours.

use super::harness::*;
use mtg_core::Zone;

#[test]
fn raise_the_past_returns_only_your_cheap_creatures() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Return all creature cards with mana value 2 or less from your graveyard to the battlefield.",
    );
    let cheap = t.card("{1}{W}", "Creature — Soldier", Some((2, 2)), "");
    let dear = t.card("{4}{W}", "Creature — Angel", Some((4, 4)), "");
    let mut g = Game::new(t);
    g.put(cheap, P0, Zone::Graveyard);
    g.put(cheap, P0, Zone::Graveyard);
    g.put(dear, P0, Zone::Graveyard);
    g.put(cheap, P1, Zone::Graveyard);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    let on_field = |card| {
        g.engine
            .state
            .battlefield()
            .iter()
            .filter(|id| g.engine.state.objects[id].card == card)
            .count()
    };
    assert_eq!(on_field(cheap), 2);
    assert_eq!(on_field(dear), 0);
    // Theirs stays put; the spell joins the Angel in your graveyard.
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
    assert_eq!(g.count(Zone::Graveyard, P0), 2);
}

#[test]
fn dusk_returns_small_creatures_to_hand() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Return all creature cards with power 2 or less from your graveyard to your hand.",
    );
    let small = t.card("{3}", "Creature — Wall", Some((0, 6)), "");
    let big = t.card("{3}", "Creature — Ogre", Some((3, 3)), "");
    let mut g = Game::new(t);
    g.put(small, P0, Zone::Graveyard);
    g.put(big, P0, Zone::Graveyard);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.cast(s, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand);
    assert_eq!(g.count(Zone::Graveyard, P0), 2);
}
