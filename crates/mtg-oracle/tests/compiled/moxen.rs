//! Chrome Mox ("Imprint — When this artifact enters, you may exile a nonartifact, nonland
//! card from your hand. {T}: Add one mana of any of the exiled card's colors.") and Mox
//! Amber ("{T}: Add one mana of any color among legendary creatures and planeswalkers you
//! control.").

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{actions::Action, choice::Answer};

const CHROME: &str = "Imprint — When this artifact enters, you may exile a nonartifact, \
                      nonland card from your hand.\n{T}: Add one mana of any of the exiled \
                      card's colors.";
const AMBER: &str =
    "{T}: Add one mana of any color among legendary creatures and planeswalkers you control.";

/// Cast Chrome Mox, imprinting the blue card (or nothing); which of a {U} and an {R}
/// spell P0 can then cast.
fn chrome(imprint: bool) -> (bool, bool) {
    let mut table = Table::default();
    let mox = table.card("{0}", "Artifact", None, CHROME);
    let blue = table.card("{3}{U}", "Sorcery", None, "Draw a card.");
    let u = table.card("{U}", "Instant", None, "You gain 1 life.");
    let r = table.card("{R}", "Instant", None, "You gain 1 life.");
    let mut game = Game::new(table);
    let mox = game.put(mox, P0, Zone::Hand);
    let blue = game.put(blue, P0, Zone::Hand);
    let u = game.put(u, P0, Zone::Hand);
    let r = game.put(r, P0, Zone::Hand);
    game.main();
    let answers = if imprint {
        vec![Answer::Bool(true), Answer::Objects(vec![blue])]
    } else {
        vec![Answer::Bool(false)]
    };
    game.act(Action::Cast { object: mox }, &[], &answers);
    assert_eq!(
        game.engine.state.objects.contains_key(&blue),
        !imprint,
        "the imprinted card left the hand"
    );
    let actions = game.until(P0, mtg_core::Step::PrecombatMain);
    (
        actions.contains(&Action::Cast { object: u }),
        actions.contains(&Action::Cast { object: r }),
    )
}

#[test]
fn chrome_mox_makes_the_imprinted_cards_colors() {
    assert_eq!(chrome(true), (true, false));
}

#[test]
fn chrome_mox_without_an_imprint_makes_nothing() {
    assert_eq!(chrome(false), (false, false));
}

/// Mox Amber beside these creatures (legendary?, color): can P0 cast a {G} and a {U}
/// spell?
fn amber(creatures: &[(bool, &str)]) -> (bool, bool) {
    let mut table = Table::default();
    let mox = table.card("{0}", "Legendary Artifact", None, AMBER);
    let g = table.card("{G}", "Instant", None, "You gain 1 life.");
    let u = table.card("{U}", "Instant", None, "You gain 1 life.");
    let made: Vec<_> = creatures
        .iter()
        .map(|(legendary, cost)| {
            let line = if *legendary {
                "Legendary Creature — Elf"
            } else {
                "Creature — Elf"
            };
            table.card(cost, line, Some((1, 1)), "")
        })
        .collect();
    let mut game = Game::new(table);
    game.put(mox, P0, Zone::Battlefield);
    for c in made {
        game.put(c, P0, Zone::Battlefield);
    }
    let g = game.put(g, P0, Zone::Hand);
    let u = game.put(u, P0, Zone::Hand);
    let actions = game.main();
    (
        actions.contains(&Action::Cast { object: g }),
        actions.contains(&Action::Cast { object: u }),
    )
}

#[test]
fn mox_amber_makes_colors_of_legendary_creatures_only() {
    assert_eq!(amber(&[(true, "{G}"), (false, "{U}")]), (true, false));
}

#[test]
fn mox_amber_without_legends_makes_nothing() {
    assert_eq!(amber(&[(false, "{G}")]), (false, false));
}
