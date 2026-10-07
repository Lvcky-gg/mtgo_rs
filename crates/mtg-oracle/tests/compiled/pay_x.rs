//! "You may pay {X}. If you do / When you do, … X …": X is chosen as it is paid.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{actions::Action, choice::Answer};

fn cast_with(text: &str, answers: &[Answer]) -> (Game, mtg_core::ObjectId, usize) {
    let mut table = Table::default();
    let creature = table.card("{1}{G}", "Creature — Bear", Some((2, 2)), text);
    let mut game = Game::new(table);
    game.lands(4);
    let id = game.put(creature, P0, Zone::Hand);
    game.main();
    let hand = game.count(Zone::Hand, P0);
    game.act(Action::Cast { object: id }, &[], answers);
    let on_battlefield = *game.engine.state.battlefield().last().unwrap();
    (game, on_battlefield, hand)
}

#[test]
fn paying_x_draws_that_many() {
    let (game, _, hand) = cast_with(
        "When this creature enters, you may pay {X}. If you do, draw X cards.",
        &[Answer::Bool(true), Answer::Number(2)],
    );
    // Cast (-1), drew two.
    assert_eq!(game.count(Zone::Hand, P0), hand + 1);
}

#[test]
fn a_when_you_do_trigger_keeps_the_x_paid() {
    let (game, creature, _) = cast_with(
        "When this creature enters, you may pay {X}. When you do, put X +1/+1 counters on this \
         creature.",
        &[Answer::Bool(true), Answer::Number(2)],
    );
    assert_eq!(
        game.engine.state.objects[&creature]
            .counters
            .get(&mtg_core::CounterKind::PlusOnePlusOne),
        Some(&2)
    );
}

#[test]
fn declining_pays_nothing() {
    let (game, _, hand) = cast_with(
        "When this creature enters, you may pay {X}. If you do, draw X cards.",
        &[Answer::Bool(false)],
    );
    assert_eq!(game.count(Zone::Hand, P0), hand - 1);
}
