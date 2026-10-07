//! "You may have it deal damage … If you do, this creature assigns no combat damage this
//! turn." — the trade between a trigger's damage and the creature's combat damage.

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::choice::Answer;

const TEXT: &str = "Whenever this creature becomes blocked, you may have it deal damage equal \
                    to its power to target creature. If you do, this creature assigns no combat \
                    damage this turn.";

fn setup() -> (Game, mtg_core::ObjectId, mtg_core::ObjectId, mtg_core::ObjectId) {
    let mut table = Table::default();
    let rider = table.card("{2}{R}", "Creature — Bear", Some((3, 3)), TEXT);
    let wall = table.card("{1}{W}", "Creature — Wall", Some((0, 4)), "");
    let bear = table.bear();
    let mut game = Game::new(table);
    let rider = game.put(rider, P0, Zone::Battlefield);
    let wall = game.put(wall, P1, Zone::Battlefield);
    let bear = game.put(bear, P1, Zone::Battlefield);
    game.main();
    (game, rider, wall, bear)
}

#[test]
fn dealing_the_trigger_damage_replaces_the_combat_damage() {
    let (mut game, rider, wall, bear) = setup();
    game.combat(
        &[rider],
        &[(wall, rider)],
        &[Target::Object(bear)],
        &[Answer::Bool(true)],
    );
    assert!(!game.engine.state.objects.contains_key(&bear), "3 damage to the bear");
    assert_eq!(
        game.engine.state.objects[&wall].damage,
        0,
        "the rider assigned no combat damage"
    );
}

#[test]
fn declining_keeps_the_combat_damage() {
    let (mut game, rider, wall, bear) = setup();
    game.combat(
        &[rider],
        &[(wall, rider)],
        &[Target::Object(bear)],
        &[Answer::Bool(false)],
    );
    assert!(game.engine.state.objects.contains_key(&bear));
    assert_eq!(game.engine.state.objects[&wall].damage, 3);
}
