//! Simian and Elvish Spirit Guide: "Exile this card from your hand: Add {R}." — a mana
//! ability of a card in hand (CR 605, 113.6j), paid for by exiling the card.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

const GUIDE: &str = "Exile this card from your hand: Add {R}.";

#[test]
fn a_guide_in_hand_pays_for_a_spell() {
    let mut table = Table::default();
    let guide = table.card("{2}{R}", "Creature — Ape Spirit", Some((2, 2)), GUIDE);
    let bolt = table.card("{R}", "Instant", None, "You gain 3 life.");
    let mut game = Game::new(table);
    let guide = game.put(guide, P0, Zone::Hand);
    let bolt = game.put(bolt, P0, Zone::Hand);
    let actions = game.main();
    assert!(
        actions.contains(&Action::Cast { object: bolt }),
        "{{R}} is affordable through the guide"
    );
    let hand = game.count(Zone::Hand, P0);
    game.cast(bolt, &[]);
    assert_eq!(game.life(P0), 23);
    assert!(!game.engine.state.objects.contains_key(&guide));
    assert_eq!(
        game.count(Zone::Hand, P0),
        hand - 2,
        "the spell and the guide"
    );
    assert_eq!(
        game.engine
            .state
            .objects_in(mtg_core::ZoneRef::shared(Zone::Exile))
            .len(),
        1,
        "the guide is exiled, not discarded"
    );
    assert_eq!(game.count(Zone::Graveyard, P0), 1, "only the spell");
}

#[test]
fn without_the_guide_the_spell_is_unaffordable() {
    let mut table = Table::default();
    let bolt = table.card("{R}", "Instant", None, "You gain 3 life.");
    let mut game = Game::new(table);
    let bolt = game.put(bolt, P0, Zone::Hand);
    assert!(!game.main().contains(&Action::Cast { object: bolt }));
}

#[test]
fn a_guide_on_the_battlefield_makes_no_mana() {
    let mut table = Table::default();
    let guide = table.card("{2}{R}", "Creature — Ape Spirit", Some((2, 2)), GUIDE);
    let bolt = table.card("{R}", "Instant", None, "You gain 3 life.");
    let mut game = Game::new(table);
    game.put(guide, P0, Zone::Battlefield);
    let bolt = game.put(bolt, P0, Zone::Hand);
    assert!(!game.main().contains(&Action::Cast { object: bolt }));
}

#[test]
fn the_elvish_guide_makes_green() {
    let mut table = Table::default();
    let guide = table.card(
        "{2}{G}",
        "Creature — Elf Spirit",
        Some((2, 2)),
        "Exile this card from your hand: Add {G}.",
    );
    let red = table.card("{R}", "Instant", None, "You gain 3 life.");
    let green = table.card("{G}", "Instant", None, "You gain 3 life.");
    let mut game = Game::new(table);
    game.put(guide, P0, Zone::Hand);
    let red = game.put(red, P0, Zone::Hand);
    let green = game.put(green, P0, Zone::Hand);
    let actions = game.main();
    assert!(actions.contains(&Action::Cast { object: green }));
    assert!(!actions.contains(&Action::Cast { object: red }));
}
