//! Arcane Denial: "Counter target spell. Its controller may draw up to two cards at the
//! beginning of the next turn's upkeep. You draw a card at the beginning of the next
//! turn's upkeep."

use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const DENIAL: &str = "Counter target spell. Its controller may draw up to two cards at the \
                      beginning of the next turn's upkeep. You draw a card at the beginning \
                      of the next turn's upkeep.";

#[test]
fn the_countered_player_draws_up_to_two_next_upkeep() {
    let mut table = Table::default();
    let denial = table.card("{0}", "Instant", None, DENIAL);
    let gift = table.card("{0}", "Sorcery", None, "You gain 3 life.");
    let filler = table.bear();
    let mut game = Game::new(table);
    for _ in 0..5 {
        game.put(filler, P0, Zone::Library);
        game.put(filler, P1, Zone::Library);
    }
    let gift = game.put(gift, P0, Zone::Hand);
    let denial = game.put(denial, P1, Zone::Hand);
    game.main();
    let before = (
        game.count(Zone::Hand, P0) - 1,
        game.count(Zone::Hand, P1) - 1,
    );
    let c = game.pending.take().unwrap();
    game.engine
        .answer(
            &game.table,
            c.id,
            Answer::Action(Action::Cast { object: gift }),
        )
        .unwrap();
    let mut asked = None;
    let mut countered = false;
    game.drive(
        |game, c| match &c.kind {
            ChoiceKind::Priority { .. }
                if c.who == P1 && !countered && !game.stack().is_empty() =>
            {
                countered = true;
                Some(Answer::Action(Action::Cast { object: denial }))
            }
            ChoiceKind::ChooseTargets { slots, .. } if c.who == P1 => {
                Some(Answer::Targets(vec![vec![slots[0][0]]]))
            }
            ChoiceKind::Confirm if c.who == P0 => Some(Answer::Bool(true)),
            ChoiceKind::ChooseX { min, max } => {
                asked = Some((c.who, *min, *max));
                Some(Answer::Number(2))
            }
            ChoiceKind::OrderTriggers { triggers, .. } => Some(Answer::Order(triggers.clone())),
            _ => None,
        },
        |game| game.engine.state.active_player == P1 && game.engine.state.step == Step::Draw,
    );
    assert_eq!(game.life(P0), 20, "countered");
    assert_eq!(
        asked,
        Some((P0, 0, 2)),
        "the countered spell's controller chooses"
    );
    assert_eq!(game.count(Zone::Hand, P0), before.0 + 2);
    assert_eq!(
        game.count(Zone::Hand, P1),
        before.1 + 2,
        "one from Arcane Denial, one from the draw step"
    );
}
