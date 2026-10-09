//! Thrasios: "{4}: Scry 1, then reveal the top card of your library. If it's a land card,
//! put it onto the battlefield tapped. Otherwise, draw a card."

use super::harness::*;
use mtg_core::{Zone, ZoneRef};

const THRASIOS: &str = "{4}: Scry 1, then reveal the top card of your library. If it's a land \
                        card, put it onto the battlefield tapped. Otherwise, draw a card.";

/// Activate with `land_on_top` deciding the top card; P0's hand size change and whether a
/// tapped land entered.
fn run(land_on_top: bool) -> (i64, bool) {
    let mut table = Table::default();
    let thrasios = table.card(
        "{G}{U}",
        "Legendary Creature — Merfolk Wizard",
        Some((1, 3)),
        THRASIOS,
    );
    let land = table.card("", "Land", None, "");
    let mut game = Game::new(table);
    game.lands(4);
    let thrasios = game.put(thrasios, P0, Zone::Battlefield);
    // After the draw step, so the land stays on top.
    game.main();
    if land_on_top {
        let top = game.put(land, P0, Zone::Library);
        let order = game
            .engine
            .state
            .zone_order
            .get_mut(&ZoneRef::of(Zone::Library, P0))
            .unwrap();
        order.retain(|o| *o != top);
        order.insert(0, top);
    }
    let hand = game.count(Zone::Hand, P0) as i64;
    let lands_before = game.engine.state.battlefield().len();
    // Keep the scried card on top.
    game.act(
        activate(thrasios, 0),
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![])],
    );
    let entered = game.engine.state.battlefield().len() > lands_before;
    let tapped_land = game.engine.state.battlefield().into_iter().any(|id| {
        let o = &game.engine.state.objects[&id];
        o.card == land && o.tapped
    });
    (
        game.count(Zone::Hand, P0) as i64 - hand,
        entered && tapped_land,
    )
}

#[test]
fn a_land_on_top_enters_tapped() {
    assert_eq!(run(true), (0, true));
}

#[test]
fn otherwise_draw_a_card() {
    assert_eq!(run(false), (1, false));
}
