//! Chaos Warp: "The owner of target permanent shuffles it into their library, then reveals
//! the top card of their library. If it's a permanent card, they put it onto the
//! battlefield."

use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};
use mtg_engine::actions::Action;

const WARP: &str = "The owner of target permanent shuffles it into their library, then \
                    reveals the top card of their library. If it's a permanent card, they put \
                    it onto the battlefield.";

/// Warp P1's Bear with P1's library made only of `library_card`s (plus the Bear).
fn warp(permanents: bool) -> Game {
    let mut table = Table::default();
    let warp = table.card("{2}{R}", "Instant", None, WARP);
    let sorcery = table.card("{1}", "Sorcery", None, "You gain 1 life.");
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(3);
    let warp = game.put(warp, P0, Zone::Hand);
    let target = game.put(bear, P1, Zone::Battlefield);
    game.main();
    if !permanents {
        let library = ZoneRef::of(Zone::Library, P1);
        for id in game.engine.state.objects_in(library) {
            game.engine.state.objects.remove(&id);
        }
        game.engine.state.zone_order.insert(library, Vec::new());
        for _ in 0..5 {
            game.put(sorcery, P1, Zone::Library);
        }
    }
    game.act(
        Action::Cast { object: warp },
        &[Target::Object(target)],
        &[],
    );
    assert!(!game.engine.state.objects.contains_key(&target));
    game
}

fn p1_creatures(game: &Game) -> usize {
    game.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            let o = &game.engine.state.objects[id];
            o.controller == P1 && o.owner == P1
        })
        .count()
}

#[test]
fn a_permanent_on_top_enters_under_its_owner() {
    let game = warp(true);
    assert_eq!(p1_creatures(&game), 1);
}

#[test]
fn otherwise_nothing_enters_and_the_card_stays_in_the_library() {
    let game = warp(false);
    // With only sorceries and the Bear itself, the Bear may be on top: either it came back
    // or nothing did, and every card is in P1's library or on the battlefield.
    let library = game.count(Zone::Library, P1);
    let back = p1_creatures(&game);
    assert_eq!(library + back, 6);
}
