//! Drannith Magistrate: "Your opponents can't cast spells from anywhere other than their
//! hands." — commanders in the command zone included.

use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::actions::Action;

const MAGISTRATE: &str = "Your opponents can't cast spells from anywhere other than their hands.";

fn setup(magistrate_owner: Option<mtg_core::PlayerId>) -> (Game, [mtg_core::ObjectId; 3]) {
    let mut table = Table::default();
    let magistrate = table.card(
        "{1}{W}",
        "Creature — Human Wizard",
        Some((1, 3)),
        MAGISTRATE,
    );
    let general = table.card("{0}", "Legendary Creature — Elf", Some((1, 1)), "");
    let other_general = table.card("{0}", "Legendary Creature — Human", Some((1, 1)), "");
    let spell = table.card("{0}", "Sorcery", None, "You gain 1 life.");
    let mut game = Game::new(table);
    if let Some(owner) = magistrate_owner {
        game.put(magistrate, owner, Zone::Battlefield);
    }
    game.engine.state.commander.commanders.insert(P1, general);
    game.engine
        .state
        .commander
        .commanders
        .insert(P0, other_general);
    let theirs = game.put(general, P1, Zone::Command);
    let mine = game.put(other_general, P0, Zone::Command);
    let in_hand = game.put(spell, P1, Zone::Hand);
    (game, [theirs, mine, in_hand])
}

#[test]
fn opponents_cannot_cast_their_commander_but_can_cast_from_hand() {
    let (mut game, [theirs, mine, in_hand]) = setup(Some(P0));
    let p0 = game.main();
    assert!(
        p0.contains(&Action::Cast { object: mine }),
        "the controller is unaffected"
    );
    let p1 = game.until(P1, Step::PrecombatMain);
    assert!(!p1.contains(&Action::Cast { object: theirs }));
    assert!(p1.contains(&Action::Cast { object: in_hand }));
}

#[test]
fn without_it_the_commander_can_be_cast() {
    let (mut game, [theirs, _, _]) = setup(None);
    game.main();
    let p1 = game.until(P1, Step::PrecombatMain);
    assert!(p1.contains(&Action::Cast { object: theirs }));
}
