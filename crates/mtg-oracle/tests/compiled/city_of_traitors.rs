//! City of Traitors: "When you play another land, sacrifice this land." — played lands only,
//! not lands put onto the battlefield by an effect.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

const CITY: &str = "{T}: Add {C}{C}.\nWhen you play another land, sacrifice this land.";

fn setup() -> (
    Game,
    mtg_core::ObjectId,
    mtg_core::ObjectId,
    mtg_core::CardId,
) {
    let mut table = Table::default();
    let city = table.card("", "Land", None, CITY);
    let land = table.card("", "Land", None, "{T}: Add {G}.");
    let mut game = Game::new(table);
    let city = game.put(city, P0, Zone::Battlefield);
    let other = game.put(land, P0, Zone::Hand);
    (game, city, other, land)
}

#[test]
fn playing_another_land_sacrifices_it() {
    let (mut game, city, other, _) = setup();
    game.main();
    game.act(Action::PlayLand { object: other }, &[], &[]);
    assert!(!game.engine.state.objects.contains_key(&city));
    assert_eq!(game.count(Zone::Graveyard, P0), 1);
}

#[test]
fn playing_the_city_itself_does_not() {
    let mut table = Table::default();
    let city = table.card("", "Land", None, CITY);
    let mut game = Game::new(table);
    let city = game.put(city, P0, Zone::Hand);
    game.main();
    game.act(Action::PlayLand { object: city }, &[], &[]);
    assert_eq!(game.count(Zone::Graveyard, P0), 0);
}

#[test]
fn a_land_put_onto_the_battlefield_is_not_played() {
    let (mut game, city, other, _) = setup();
    let spell = game.table.card(
        "{0}",
        "Sorcery",
        None,
        "You may put a land card from your hand onto the battlefield.",
    );
    let spell = game.put(spell, P0, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: spell },
        &[],
        &[
            mtg_engine::choice::Answer::Bool(true),
            mtg_engine::choice::Answer::Objects(vec![other]),
        ],
    );
    assert!(
        !game.engine.state.objects.contains_key(&other),
        "the land left the hand"
    );
    assert!(game.engine.state.objects.contains_key(&city));
}
