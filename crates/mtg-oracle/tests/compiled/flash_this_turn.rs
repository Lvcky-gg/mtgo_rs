//! Borne Upon a Wind: "You may cast spells this turn as though they had flash." — a
//! permission that outlives the spell, for its caster, this turn only.

use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::actions::Action;

const BORNE: &str = "You may cast spells this turn as though they had flash.\nDraw a card.";

fn game(cast_borne: bool) -> (Game, mtg_core::ObjectId) {
    let mut table = Table::default();
    let borne = table.card("{0}", "Instant", None, BORNE);
    let sorcery = table.card("{0}", "Sorcery", None, "You gain 1 life.");
    let mut game = Game::new(table);
    let borne = game.put(borne, P0, Zone::Hand);
    let sorcery = game.put(sorcery, P0, Zone::Hand);
    game.main();
    if cast_borne {
        game.cast(borne, &[]);
    }
    (game, sorcery)
}

#[test]
fn a_sorcery_can_be_cast_in_combat_this_turn() {
    let (mut game, sorcery) = game(true);
    let actions = game.until(P0, Step::BeginCombat);
    assert!(actions.contains(&Action::Cast { object: sorcery }));
}

#[test]
fn not_without_it() {
    let (mut game, sorcery) = game(false);
    let actions = game.until(P0, Step::BeginCombat);
    assert!(!actions.contains(&Action::Cast { object: sorcery }));
}

#[test]
fn the_permission_ends_with_the_turn() {
    let (mut game, sorcery) = game(true);
    game.until(P0, Step::End);
    // P0's next turn.
    game.until(P1, Step::PrecombatMain);
    let actions = game.until(P0, Step::BeginCombat);
    assert!(!actions.contains(&Action::Cast { object: sorcery }));
}
