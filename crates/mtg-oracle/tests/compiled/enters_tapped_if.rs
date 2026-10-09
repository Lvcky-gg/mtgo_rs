//! "If you control two or more other lands, this land enters tapped."

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

fn tapped_with(other_lands: usize) -> bool {
    let mut table = Table::default();
    let land = table.card(
        "",
        "Land",
        None,
        "If you control two or more other lands, this land enters tapped.\n{T}: Add {G}.",
    );
    let basic = table.card("", "Land", None, "{T}: Add {G}.");
    let mut game = Game::new(table);
    for _ in 0..other_lands {
        game.put(basic, P0, Zone::Battlefield);
    }
    let land = game.put(land, P0, Zone::Hand);
    game.main();
    game.act(Action::PlayLand { object: land }, &[], &[]);
    let id = game
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| game.engine.state.objects[id].card != basic)
        .unwrap();
    game.engine.state.objects[&id].tapped
}

#[test]
fn untapped_with_one_other_land() {
    assert!(!tapped_with(1));
}

#[test]
fn tapped_with_two_other_lands() {
    assert!(tapped_with(2));
}
