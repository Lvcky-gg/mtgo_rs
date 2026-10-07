//! Kinship: at upkeep, if the top card of your library shares a creature type with this
//! creature, you may reveal it for the payoff.

use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::choice::Answer;

const TEXT: &str = "Kinship — At the beginning of your upkeep, you may look at the top card of \
                    your library. If it shares a creature type with this creature, you may \
                    reveal it. If you do, you gain 4 life.";

/// A game whose top library card has this type line, with the kinship creature in play.
fn game_with_top(top_type: &str) -> Game {
    game_with_top_text(top_type, "")
}

fn game_with_top_text(top_type: &str, top_text: &str) -> Game {
    let mut table = Table::default();
    let graybeard = table.card("{3}{G}", "Creature — Elf Wizard", Some((4, 4)), TEXT);
    let top = table.card("{G}", top_type, Some((1, 1)), top_text);
    let mut game = Game::new(table);
    game.put(graybeard, P0, Zone::Battlefield);
    let id = game.put(top, P0, Zone::Library);
    let library = game
        .engine
        .state
        .zone_order
        .get_mut(&ZoneRef::of(Zone::Library, P0))
        .unwrap();
    library.retain(|o| *o != id);
    library.insert(0, id);
    game
}

#[test]
fn a_shared_creature_type_on_top_pays_off() {
    let mut game = game_with_top("Creature — Elf");
    play_to_main(&mut game, &[Answer::Bool(true)]);
    assert_eq!(game.life(P0), 24);
}

#[test]
fn no_shared_type_no_payoff() {
    let mut game = game_with_top("Creature — Goblin");
    play_to_main(&mut game, &[Answer::Bool(true)]);
    assert_eq!(game.life(P0), 20);
}

#[test]
fn a_changeling_on_top_shares_every_type() {
    let mut game = game_with_top_text("Creature — Shapeshifter", "Changeling");
    play_to_main(&mut game, &[Answer::Bool(true)]);
    assert_eq!(game.life(P0), 24);
}

#[test]
fn declining_to_reveal_gives_nothing() {
    let mut game = game_with_top("Creature — Elf");
    play_to_main(&mut game, &[Answer::Bool(false)]);
    assert_eq!(game.life(P0), 20);
}

/// From the start of P0's turn to its precombat main phase, answering the questions on the
/// way with `answers` in order, then each question's default.
fn play_to_main(game: &mut Game, answers: &[Answer]) {
    use mtg_engine::{Progress, choice::ChoiceKind};
    let mut answers = answers.iter().cloned();
    loop {
        match game.engine.advance(&game.table) {
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended"),
            Progress::NeedsChoice(c) => {
                if let ChoiceKind::Priority { .. } = c.kind {
                    if game.engine.state.step == mtg_core::Step::PrecombatMain
                        && game.stack().is_empty()
                    {
                        game.pending = Some(c);
                        return;
                    }
                    game.engine.answer(&game.table, c.id, Answer::Pass).unwrap();
                    continue;
                }
                let answer = answers
                    .next()
                    .or_else(|| c.default.clone())
                    .expect("an answer");
                game.engine.answer(&game.table, c.id, answer).unwrap();
            }
        }
    }
}
