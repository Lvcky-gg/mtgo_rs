//! "If you control a commander, you may cast this spell without paying its mana cost."
//! (Fierce Guardianship, Deflecting Swat, Deadly Rollick) and "If an opponent cast three
//! or more spells this turn, you may pay {0} rather than pay this spell's mana cost."
//! (Mindbreak Trap).

use super::harness::*;
use mtg_core::{ObjectId, Zone};
use mtg_engine::actions::Action;

const COMMANDER_FREE: &str =
    "If you control a commander, you may cast this spell without paying its mana cost.";
const TRAP: &str = "If an opponent cast three or more spells this turn, you may pay {0} rather \
                    than pay this spell's mana cost.";

fn alternative(actions: &[Action], object: ObjectId) -> Option<Action> {
    actions
        .iter()
        .find(|a| matches!(a, Action::CastAlternative { object: o, .. } if *o == object))
        .cloned()
}

#[test]
fn the_printed_cards_compile() {
    let mut table = Table::default();
    table.card(
        "{2}{U}",
        "Instant",
        None,
        &format!("{COMMANDER_FREE}\nCounter target noncreature spell."),
    );
    table.card(
        "{2}{U}{U}",
        "Instant",
        None,
        &format!("{TRAP}\nExile any number of target spells."),
    );
}

/// A {9} instant that draws a card; `commander` puts a commander P0 controls on the
/// battlefield (owned by `owner`).
fn commander_game(commander: Option<mtg_core::PlayerId>) -> (Game, ObjectId) {
    let mut table = Table::default();
    let spell = table.card(
        "{9}",
        "Instant",
        None,
        &format!("{COMMANDER_FREE}\nDraw a card."),
    );
    let general = table.card("{2}", "Legendary Creature — Human", Some((2, 2)), "");
    let mut game = Game::new(table);
    if let Some(owner) = commander {
        game.engine
            .state
            .commander
            .commanders
            .insert(owner, general);
        let id = game.put(general, owner, Zone::Battlefield);
        game.engine.state.objects.get_mut(&id).unwrap().controller = mtg_core::PlayerId(0);
    }
    let spell = game.put(spell, P0, Zone::Hand);
    (game, spell)
}

#[test]
fn with_your_commander_it_is_cast_for_free() {
    let (mut game, spell) = commander_game(Some(P0));
    let actions = game.main();
    assert!(
        !actions.contains(&Action::Cast { object: spell }),
        "{{9}} is unaffordable"
    );
    let cast = alternative(&actions, spell).expect("free cast offered");
    let hand = game.count(Zone::Hand, P0);
    game.act(cast, &[], &[]);
    assert_eq!(game.count(Zone::Hand, P0), hand, "spell left, card drawn");
    assert_eq!(game.count(Zone::Graveyard, P0), 1);
}

#[test]
fn another_players_commander_you_control_counts() {
    let (mut game, spell) = commander_game(Some(P1));
    assert!(alternative(&game.main(), spell).is_some());
}

#[test]
fn without_a_commander_there_is_no_free_cast() {
    let (mut game, spell) = commander_game(None);
    assert!(alternative(&game.main(), spell).is_none());
}

#[test]
fn a_commander_card_in_the_command_zone_does_not_count() {
    let mut table = Table::default();
    let spell = table.card(
        "{9}",
        "Instant",
        None,
        &format!("{COMMANDER_FREE}\nDraw a card."),
    );
    let general = table.card("{2}", "Legendary Creature — Human", Some((2, 2)), "");
    let mut game = Game::new(table);
    game.engine.state.commander.commanders.insert(P0, general);
    game.put(general, P0, Zone::Command);
    let spell = game.put(spell, P0, Zone::Hand);
    assert!(alternative(&game.main(), spell).is_none());
}

fn trap_game(opponent_spells: u32) -> (Game, ObjectId) {
    let mut table = Table::default();
    let spell = table.card("{9}", "Instant", None, &format!("{TRAP}\nDraw a card."));
    let mut game = Game::new(table);
    let spell = game.put(spell, P0, Zone::Hand);
    game.main();
    game.engine
        .state
        .spells_by_player
        .insert(P1, opponent_spells);
    // Re-offer priority with the opponent's count in place.
    game.pending = None;
    (game, spell)
}

/// Pass priority once so the legal actions are recomputed, and return P0's next offer.
fn reoffer(game: &mut Game) -> Vec<Action> {
    game.until(P0, mtg_core::Step::PostcombatMain)
}

#[test]
fn mindbreak_trap_costs_zero_after_three_opponent_spells() {
    let (mut game, spell) = trap_game(3);
    let actions = reoffer(&mut game);
    let cast = alternative(&actions, spell).expect("{0} alternative offered");
    let hand = game.count(Zone::Hand, P0);
    game.act(cast, &[], &[]);
    assert_eq!(game.count(Zone::Hand, P0), hand);
}

#[test]
fn mindbreak_trap_needs_three() {
    let (mut game, spell) = trap_game(2);
    let actions = reoffer(&mut game);
    assert!(alternative(&actions, spell).is_none());
}
