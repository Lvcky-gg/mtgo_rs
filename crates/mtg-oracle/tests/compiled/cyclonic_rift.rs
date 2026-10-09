//! Cyclonic Rift: "Return target nonland permanent you don't control to its owner's
//! hand. Overload {6}{U}" — overloaded, each one instead (CR 702.96).

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

const RIFT: &str =
    "Return target nonland permanent you don't control to its owner's hand.\nOverload {6}{U}";

fn setup() -> (Game, mtg_core::ObjectId, [mtg_core::ObjectId; 3]) {
    let mut table = Table::default();
    let rift = table.card("{1}{U}", "Instant", None, RIFT);
    let bear = table.bear();
    let rock = table.card("{1}", "Artifact", None, "");
    let land = table.card("", "Land", None, "");
    let mut game = Game::new(table);
    game.lands(7);
    let rift = game.put(rift, P0, Zone::Hand);
    let mine = game.put(bear, P0, Zone::Battlefield);
    let theirs = game.put(bear, P1, Zone::Battlefield);
    let their_rock = game.put(rock, P1, Zone::Battlefield);
    game.put(land, P1, Zone::Battlefield);
    (game, rift, [mine, theirs, their_rock])
}

#[test]
fn overloaded_it_returns_each_of_their_nonland_permanents() {
    let (mut game, rift, [mine, theirs, rock]) = setup();
    let actions = game.main();
    let overload = actions
        .iter()
        .find(|a| matches!(a, Action::CastAlternative { object, .. } if *object == rift))
        .expect("overload offered")
        .clone();
    let p1_hand = game.count(Zone::Hand, P1);
    game.act(overload, &[], &[]);
    let alive = |id| game.engine.state.objects.contains_key(&id);
    assert!(alive(mine), "P0's own permanent stays");
    assert!(!alive(theirs) && !alive(rock));
    assert_eq!(game.count(Zone::Hand, P1), p1_hand + 2, "lands stay");
}

#[test]
fn cast_normally_it_returns_one_target() {
    let (mut game, rift, [mine, theirs, rock]) = setup();
    game.main();
    game.act(
        Action::Cast { object: rift },
        &[Target::Object(theirs)],
        &[],
    );
    let alive = |id| game.engine.state.objects.contains_key(&id);
    assert!(alive(mine) && alive(rock));
    assert!(!alive(theirs));
}
