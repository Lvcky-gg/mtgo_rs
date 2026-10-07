//! "You may play lands from your graveyard." (Crucible of Worlds)

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

#[test]
fn a_land_in_the_graveyard_can_be_played_as_the_land_for_the_turn() {
    let mut table = Table::default();
    let crucible = table.card(
        "{3}",
        "Artifact",
        None,
        "You may play lands from your graveyard.",
    );
    let mountain = table.mountain();
    let mut game = Game::new(table);
    game.put(crucible, P0, Zone::Battlefield);
    let first = game.put(mountain, P0, Zone::Graveyard);
    let second = game.put(mountain, P0, Zone::Graveyard);
    let actions = game.main();
    assert!(actions.contains(&Action::PlayLand { object: first }));
    game.act(Action::PlayLand { object: first }, &[], &[]);
    assert_eq!(game.count(Zone::Graveyard, P0), 1);
    let actions = game.main();
    assert!(
        !actions.contains(&Action::PlayLand { object: second }),
        "one land per turn"
    );
}

#[test]
fn without_the_permission_it_stays_there() {
    let mut table = Table::default();
    let mountain = table.mountain();
    let mut game = Game::new(table);
    let land = game.put(mountain, P0, Zone::Graveyard);
    let actions = game.main();
    assert!(!actions.contains(&Action::PlayLand { object: land }));
}
