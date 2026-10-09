//! Seedborn Muse: "Untap all permanents you control during each other player's untap
//! step."

use super::harness::*;
use mtg_core::{Step, Zone};

const SEEDBORN: &str = "Untap all permanents you control during each other player's untap step.";

/// P0's tapped rock, as P1's turn reaches its upkeep.
fn rock_tapped_in_p1s_turn(text: &str) -> bool {
    let mut table = Table::default();
    let muse = table.card("{3}{G}{G}", "Creature — Spirit", Some((2, 4)), text);
    let rock = table.card("{1}", "Artifact", None, "");
    let mut game = Game::new(table);
    game.put(muse, P0, Zone::Battlefield);
    let rock = game.put(rock, P0, Zone::Battlefield);
    game.main();
    game.engine.state.objects.get_mut(&rock).unwrap().tapped = true;
    game.until(P1, Step::Upkeep);
    game.engine.state.objects[&rock].tapped
}

#[test]
fn your_permanents_untap_in_other_players_untap_steps() {
    assert!(!rock_tapped_in_p1s_turn(SEEDBORN));
}

#[test]
fn without_it_they_stay_tapped() {
    assert!(rock_tapped_in_p1s_turn(""));
}
