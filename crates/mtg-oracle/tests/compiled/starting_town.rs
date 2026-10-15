//! Starting Town: "This land enters tapped unless it's your first, second, or third turn of
//! the game."

use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::actions::Action;

const TOWN: &str = "This land enters tapped unless it's your first, second, or third turn of \
                    the game.\n{T}: Add {C}.\n{T}, Pay 1 life: Add one mana of any color.";

fn play_on_turn(n: u32) -> bool {
    let mut t = Table::default();
    let town = t.card("", "Land — Town", None, TOWN);
    let mut g = Game::new(t);
    let town_obj = g.put(town, P0, Zone::Hand);
    g.main();
    assert_eq!(
        g.engine.state.turns_taken.get(&P0),
        Some(&1),
        "this is P0's first turn"
    );
    g.engine.state.turns_taken.insert(P0, n);
    g.act(Action::PlayLand { object: town_obj }, &[], &[]);
    let id = g.find(town).unwrap();
    g.engine.state.objects[&id].tapped
}

#[test]
fn untapped_on_your_first_three_turns_only() {
    assert!(!play_on_turn(1));
    assert!(!play_on_turn(3));
    assert!(play_on_turn(4));
}

#[test]
fn each_players_turns_are_counted() {
    let mut t = Table::default();
    let _ = t.card("", "Land — Town", None, TOWN);
    let mut g = Game::new(t);
    g.main();
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    assert_eq!(g.engine.state.turns_taken.get(&P0), Some(&2));
    assert_eq!(g.engine.state.turns_taken.get(&P1), Some(&1));
}
