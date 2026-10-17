//! "You may cast this card from your graveyard", "… by paying {2}{W} rather than paying its
//! mana cost", "… by discarding a card in addition to paying its other costs": the card's
//! own permission. Unlike flashback it isn't exiled afterwards.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

#[test]
fn a_creature_is_cast_from_the_graveyard_and_can_come_back_again() {
    let mut t = Table::default();
    let goblin = t.card(
        "{1}{R}",
        "Creature — Goblin",
        Some((2, 1)),
        "You may cast this card from your graveyard.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let card = g.put(goblin, P0, Zone::Graveyard);
    assert!(g.main().contains(&Action::Cast { object: card }));
    g.cast(card, &[]);
    let on = g.find(goblin).expect("on the battlefield");
    assert_eq!(g.engine.state.objects[&on].zone.zone, Zone::Battlefield);
}

#[test]
fn not_without_the_mana() {
    let mut t = Table::default();
    let goblin = t.card(
        "{1}{R}",
        "Creature — Goblin",
        Some((2, 1)),
        "You may cast this card from your graveyard.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let card = g.put(goblin, P0, Zone::Graveyard);
    assert!(!g.main().contains(&Action::Cast { object: card }));
}

#[test]
fn rather_than_its_mana_cost() {
    let mut t = Table::default();
    let angel = t.card(
        "{5}{W}",
        "Creature — Angel",
        Some((4, 4)),
        "You may cast this card from your graveyard by paying {W} rather than paying its mana cost.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let card = g.put(angel, P0, Zone::Graveyard);
    assert!(g.main().contains(&Action::Cast { object: card }));
    g.cast(card, &[]);
    let on = g.find(angel).expect("cast");
    assert_eq!(g.engine.state.objects[&on].zone.zone, Zone::Battlefield);
}

#[test]
fn in_addition_a_card_is_discarded() {
    let mut t = Table::default();
    let horror = t.card(
        "{B}",
        "Creature — Horror",
        Some((2, 2)),
        "You may cast this card from your graveyard by discarding a card in addition to paying its other costs.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let card = g.put(horror, P0, Zone::Graveyard);
    assert!(g.main().contains(&Action::Cast { object: card }));
    // The turn's draw is the only card in hand.
    assert_eq!(g.count(Zone::Hand, P0), 1);
    g.cast(card, &[]);
    assert_eq!(g.count(Zone::Hand, P0), 0, "discarded");
    let on = g.find(horror).expect("cast");
    assert_eq!(g.engine.state.objects[&on].zone.zone, Zone::Battlefield);
}

#[test]
fn a_sorcery_returns_to_the_graveyard_not_exile() {
    let mut t = Table::default();
    let spell = t.card(
        "{1}",
        "Sorcery",
        None,
        "Draw a card.\nYou may cast this card from your graveyard.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let card = g.put(spell, P0, Zone::Graveyard);
    assert!(g.main().contains(&Action::Cast { object: card }));
    g.cast(card, &[]);
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn squee_is_cast_from_exile_or_the_graveyard() {
    for zone in [Zone::Graveyard, Zone::Exile] {
        let mut t = Table::default();
        let squee = t.card(
            "{1}{R}{R}",
            "Legendary Creature — Goblin",
            Some((1, 1)),
            "You may cast this card from your graveyard or from exile.",
        );
        let mut g = Game::new(t);
        g.lands(3);
        let card = g.put(squee, P0, zone);
        assert!(
            g.main().contains(&Action::Cast { object: card }),
            "{zone:?}"
        );
        g.cast(card, &[]);
        let on = g.find(squee).expect("cast");
        assert_eq!(g.engine.state.objects[&on].zone.zone, Zone::Battlefield);
    }
}

#[test]
fn only_its_owner_casts_it_from_exile() {
    let mut t = Table::default();
    let squee = t.card(
        "{1}{R}{R}",
        "Legendary Creature — Goblin",
        Some((1, 1)),
        "You may cast this card from exile.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let card = g.put(squee, P1, Zone::Exile);
    assert!(!g.main().contains(&Action::Cast { object: card }));
}
