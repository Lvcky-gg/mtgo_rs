use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};

const TATARU: &str = "When ~ enters, you draw a card and target opponent may draw a card.\nScions' Secretary — Whenever an opponent draws a card, if it isn't that player's turn, create a tapped Treasure token. This ability triggers only once each turn.";

fn treasures(g: &Game) -> usize {
    g.engine
        .state
        .objects
        .values()
        .filter(|o| o.zone.zone == Zone::Battlefield && o.is_token)
        .count()
}

#[test]
fn entering_offers_the_opponent_a_draw_and_makes_a_tapped_treasure() {
    let mut t = Table::default();
    let tataru = t.card("{0}", "Creature", Some((0, 3)), TATARU);
    let mut g = Game::new(t);
    let tataru = g.put(tataru, P0, Zone::Hand);
    g.main();
    let before = g.count(Zone::Hand, P1);
    g.act(
        Action::Cast { object: tataru },
        &[Target::Player(P1)],
        &[Answer::Bool(true)],
    );
    assert_eq!(g.count(Zone::Hand, P1), before + 1);
    assert_eq!(treasures(&g), 1);
    assert!(
        g.engine
            .state
            .objects
            .values()
            .filter(|o| o.is_token)
            .all(|o| o.tapped && o.controller == P0)
    );
}

#[test]
fn declining_the_draw_does_not_make_a_treasure() {
    let mut t = Table::default();
    let tataru = t.card("{0}", "Creature", Some((0, 3)), TATARU);
    let mut g = Game::new(t);
    let tataru = g.put(tataru, P0, Zone::Hand);
    g.main();
    let before = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: tataru },
        &[Target::Player(P1)],
        &[Answer::Bool(false)],
    );
    assert_eq!(g.count(Zone::Hand, P0), before);
    assert_eq!(treasures(&g), 0);
}

#[test]
fn own_turn_draws_do_not_consume_the_once_per_turn_limit() {
    let mut t = Table::default();
    let tataru = t.card("{0}", "Creature", Some((0, 3)), TATARU);
    let draw = t.card("{0}", "Instant", None, "Draw two cards.");
    let mut g = Game::new(t);
    g.put(tataru, P0, Zone::Battlefield);
    let first = g.put(draw, P1, Zone::Hand);
    let second = g.put(draw, P1, Zone::Hand);
    let third = g.put(draw, P1, Zone::Hand);
    g.main();
    let choice = g.pending.take().unwrap();
    g.engine.answer(&g.table, choice.id, Answer::Pass).unwrap();
    g.drive(|_, _| None, |_| true);
    g.act(Action::Cast { object: first }, &[], &[]);
    assert_eq!(treasures(&g), 1, "a draw batch triggers only once");
    g.until(P1, Step::PrecombatMain);
    g.act(Action::Cast { object: second }, &[], &[]);
    assert_eq!(treasures(&g), 1, "opponent's own turn does not trigger");
    g.until(P0, Step::PrecombatMain);
    let choice = g.pending.take().unwrap();
    g.engine.answer(&g.table, choice.id, Answer::Pass).unwrap();
    g.drive(|_, _| None, |_| true);
    g.act(Action::Cast { object: third }, &[], &[]);
    assert_eq!(treasures(&g), 2, "a later turn resets the limit");
}
