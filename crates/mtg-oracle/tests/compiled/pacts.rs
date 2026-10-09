//! The Pacts: "At the beginning of your next upkeep, pay {…}. If you don't, you lose the
//! game." — and "you win the game" / "you lose the game" (CR 104.2a, 104.3e).

use super::harness::*;
use mtg_core::{PlayerId, Step, Zone};
use mtg_engine::{
    Progress,
    choice::{Answer, ChoiceKind},
};

const PACT: &str =
    "Draw a card.\nAt the beginning of your next upkeep, pay {1}. If you don't, you lose the game.";

/// Cast the free Pact in P0's main phase, then play on to P0's next upkeep, answering the
/// payment with `pay`. Returns the winners if the game ended there.
fn pact(pay: bool, lands: usize) -> (Game, Option<Vec<PlayerId>>) {
    let mut table = Table::default();
    let spell = table.card("{0}", "Instant", None, PACT);
    let mut game = Game::new(table);
    game.lands(lands);
    let spell = game.put(spell, P0, Zone::Hand);
    game.main();
    game.cast(spell, &[]);
    let mut asked = false;
    for _ in 0..10_000 {
        match game.engine.advance(&game.table) {
            Progress::Continue => {}
            Progress::GameOver { winners } => {
                assert_eq!(asked, lands > 0, "asked only when able to pay");
                return (game, Some(winners));
            }
            Progress::NeedsChoice(c) => {
                let answer = match c.kind {
                    ChoiceKind::Confirm => {
                        assert_eq!(c.who, P0);
                        assert_eq!(game.engine.state.step, Step::Upkeep);
                        assert_eq!(game.engine.state.active_player, P0);
                        asked = true;
                        Answer::Bool(pay)
                    }
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                game.engine.answer(&game.table, c.id, answer).unwrap();
                if asked
                    && game.engine.state.step == Step::Draw
                    && game.engine.state.active_player == P0
                {
                    return (game, None);
                }
            }
        }
    }
    panic!("never reached P0's next upkeep");
}

#[test]
fn paying_at_the_next_upkeep_keeps_you_in_the_game() {
    let (game, over) = pact(true, 1);
    assert_eq!(over, None);
    assert!(!game.engine.state.players[&P0].has_lost);
}

#[test]
fn declining_loses_the_game() {
    let (_, over) = pact(false, 1);
    assert_eq!(over, Some(vec![P1]));
}

#[test]
fn unable_to_pay_loses_without_being_asked() {
    let (_, over) = pact(true, 0);
    assert_eq!(over, Some(vec![P1]));
}

#[test]
fn you_win_the_game() {
    let mut table = Table::default();
    let oracle = table.card(
        "{0}",
        "Creature — Merfolk Wizard",
        Some((1, 3)),
        "When this creature enters, you win the game.",
    );
    let mut game = Game::new(table);
    let oracle = game.put(oracle, P0, Zone::Hand);
    game.main();
    let c = game.pending.take().unwrap();
    game.engine
        .answer(
            &game.table,
            c.id,
            Answer::Action(mtg_engine::actions::Action::Cast { object: oracle }),
        )
        .unwrap();
    for _ in 0..1_000 {
        match game.engine.advance(&game.table) {
            Progress::GameOver { winners } => {
                assert_eq!(winners, vec![P0]);
                return;
            }
            Progress::NeedsChoice(c) => {
                let a = c.default.clone().unwrap_or(Answer::Pass);
                game.engine.answer(&game.table, c.id, a).unwrap();
            }
            Progress::Continue => {}
        }
    }
    panic!("the game did not end");
}
