//! Thassa's Oracle: look at the top X cards, where X is your devotion to blue, keep up to
//! one on top; "If X is greater than or equal to the number of cards in your library, you
//! win the game."

use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::{Progress, actions::Action, choice::Answer};

const ORACLE: &str = "When this creature enters, look at the top X cards of your library, where \
                      X is your devotion to blue. Put up to one of them on top of your library \
                      and the rest on the bottom of your library in a random order. If X is \
                      greater than or equal to the number of cards in your library, you win \
                      the game.";

/// Cast the Oracle ({U}{U}: devotion 2) with `library` cards left; the winners if the game
/// ended.
fn oracle(library: usize) -> Option<Vec<mtg_core::PlayerId>> {
    let mut table = Table::default();
    let oracle = table.card("{U}{U}", "Creature — Merfolk Wizard", Some((1, 3)), ORACLE);
    let mut game = Game::new(table);
    game.lands(2);
    let oracle_card = oracle;
    let oracle = game.put(oracle, P0, Zone::Hand);
    game.main();
    let deck = game.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
    for id in deck.into_iter().skip(library) {
        game.engine.state.objects.remove(&id);
        if let Some(order) = game
            .engine
            .state
            .zone_order
            .get_mut(&ZoneRef::of(Zone::Library, P0))
        {
            order.retain(|o| *o != id);
        }
    }
    let c = game.pending.take().unwrap();
    game.engine
        .answer(
            &game.table,
            c.id,
            Answer::Action(Action::Cast { object: oracle }),
        )
        .unwrap();
    for _ in 0..1_000 {
        match game.engine.advance(&game.table) {
            Progress::GameOver { winners } => return Some(winners),
            Progress::NeedsChoice(c) => {
                // The Oracle is out and its trigger has resolved without ending the game.
                let oracle_out = game
                    .engine
                    .state
                    .battlefield()
                    .iter()
                    .any(|id| game.engine.state.objects[id].card == oracle_card);
                if matches!(c.kind, mtg_engine::choice::ChoiceKind::Priority { .. })
                    && oracle_out
                    && game.stack().is_empty()
                {
                    return None;
                }
                let a = c.default.clone().unwrap_or(Answer::Pass);
                game.engine.answer(&game.table, c.id, a).unwrap();
            }
            Progress::Continue => {}
        }
    }
    panic!("never settled");
}

#[test]
fn devotion_at_least_the_library_wins() {
    assert_eq!(oracle(1), Some(vec![P0]));
}

#[test]
fn an_empty_library_wins() {
    assert_eq!(oracle(0), Some(vec![P0]));
}

#[test]
fn a_bigger_library_does_not() {
    assert_eq!(oracle(5), None);
}
