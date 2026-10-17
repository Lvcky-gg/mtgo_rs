//! "Each spell costs {3} more to cast except during its controller's turn." (Defense
//! Grid): only a spell cast on another player's turn pays more.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

const GRID: &str = "Each spell costs {3} more to cast except during its controller's turn.";

fn generic(g: &Game, object: mtg_core::ObjectId) -> u32 {
    mtg_engine::cost::total_cost(&g.engine.state, &g.table, object, P0)
        .unwrap()
        .symbols
        .iter()
        .map(|s| match s {
            mtg_core::ManaSymbol::Generic(n) => u32::from(*n),
            _ => 0,
        })
        .sum()
}

#[test]
fn a_spell_on_its_controllers_turn_costs_the_same() {
    let mut t = Table::default();
    let grid = t.card("{2}", "Artifact", None, GRID);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(grid, P1, Zone::Battlefield);
    g.lands(2);
    let bear = g.put(bear, P0, Zone::Hand);
    assert!(g.main().contains(&Action::Cast { object: bear }));
    assert_eq!(generic(&g, bear), 1);
}

#[test]
fn a_spell_on_another_players_turn_costs_three_more() {
    let mut t = Table::default();
    let grid = t.card("{2}", "Artifact", None, GRID);
    let bolt = t.card("{1}{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.put(grid, P0, Zone::Battlefield);
    let bolt = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.engine.state.active_player = P1;
    assert_eq!(generic(&g, bolt), 4);
}
