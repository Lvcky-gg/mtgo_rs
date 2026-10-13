use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_engine::{Progress, actions::Action, choice::Answer};

const FORTUNE: &str = "Take an extra turn after this one. At the beginning of that turn's end step, you lose the game.";

fn move_to_turn(g: &mut Game, turn: u32) {
    let choice = g.pending.take().unwrap();
    g.engine.answer(&g.table, choice.id, Answer::Pass).unwrap();
    g.drive(
        |_, _| None,
        |g| g.engine.state.turn == turn && g.engine.state.step == Step::PrecombatMain,
    );
}

fn loses_at_end(g: &mut Game, turn: u32) {
    let choice = g.pending.take().unwrap();
    g.engine.answer(&g.table, choice.id, Answer::Pass).unwrap();
    for _ in 0..1000 {
        match g.engine.advance(&g.table) {
            Progress::GameOver { winners } => {
                assert_eq!(winners, vec![P1]);
                assert_eq!(g.engine.state.turn, turn);
                assert_eq!(g.engine.state.step, Step::End);
                return;
            }
            Progress::NeedsChoice(c) => {
                g.engine
                    .answer(&g.table, c.id, c.default.unwrap_or(Answer::Pass))
                    .unwrap();
            }
            Progress::Continue => {}
        }
    }
    panic!("loss did not occur at the extra turn's end step");
}

#[test]
fn loss_waits_for_the_specific_extra_turns_end_step() {
    let mut t = Table::default();
    let fortune = t.card("{0}", "Instant", None, FORTUNE);
    let mut g = Game::new(t);
    let spell = g.put(fortune, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    move_to_turn(&mut g, 3);
    assert_eq!(g.engine.state.active_player, P0);
    assert!(g.engine.state.active_extra_turn.is_some());
    loses_at_end(&mut g, 3);
}

#[test]
fn a_more_recent_extra_turn_is_taken_before_the_fortune_turn() {
    let mut t = Table::default();
    let fortune = t.card("{0}", "Instant", None, FORTUNE);
    let extra = t.card("{0}", "Sorcery", None, "Take an extra turn after this one.");
    let mut g = Game::new(t);
    let fortune = g.put(fortune, P0, Zone::Hand);
    let extra = g.put(extra, P0, Zone::Hand);
    g.main();
    g.cast(fortune, &[]);
    g.cast(extra, &[]);
    move_to_turn(&mut g, 3);
    assert!(g.engine.state.active_extra_turn.is_none());
    move_to_turn(&mut g, 4);
    assert!(g.engine.state.active_extra_turn.is_some());
    loses_at_end(&mut g, 4);
}

#[test]
fn skipping_the_fortune_turn_does_not_attach_its_loss_to_a_normal_turn() {
    let mut t = Table::default();
    let fortune = t.card("{0}", "Instant", None, FORTUNE);
    let skip = t.card("{0}", "Sorcery", None, "You skip your next turn.");
    let mut g = Game::new(t);
    let fortune = g.put(fortune, P0, Zone::Hand);
    let skip = g.put(skip, P0, Zone::Hand);
    g.main();
    g.cast(fortune, &[]);
    g.cast(skip, &[]);
    move_to_turn(&mut g, 3);
    assert_eq!(g.engine.state.active_player, P1);
    move_to_turn(&mut g, 4);
    assert_eq!(g.engine.state.active_player, P0);
    assert!(g.engine.state.active_extra_turn.is_none());
    assert!(!g.engine.state.player(P0).has_lost);
}

#[test]
fn the_delayed_loss_is_a_counterable_trigger() {
    let mut t = Table::default();
    let fortune = t.card("{0}", "Instant", None, FORTUNE);
    let counter = t.card(
        "{0}",
        "Instant",
        None,
        "Counter target activated or triggered ability.",
    );
    let mut g = Game::new(t);
    let fortune = g.put(fortune, P0, Zone::Hand);
    let counter = g.put(counter, P0, Zone::Hand);
    g.main();
    g.cast(fortune, &[]);
    let choice = g.pending.take().unwrap();
    g.engine.answer(&g.table, choice.id, Answer::Pass).unwrap();
    g.drive(
        |_, _| None,
        |g| g.engine.state.turn == 3 && g.engine.state.step == Step::End && !g.stack().is_empty(),
    );
    let loss = g.stack()[0];
    g.cast(counter, &[Target::Object(loss)]);
    g.until(P1, Step::PrecombatMain);
    assert!(!g.engine.state.player(P0).has_lost);
}

#[test]
fn countering_the_spell_creates_neither_an_extra_turn_nor_a_loss_trigger() {
    let mut t = Table::default();
    let fortune = t.card("{0}", "Instant", None, FORTUNE);
    let counter = t.card("{0}", "Instant", None, "Counter target spell.");
    let mut g = Game::new(t);
    let fortune = g.put(fortune, P0, Zone::Hand);
    let counter = g.put(counter, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: fortune }, &[]);
    let spell = g.stack()[0];
    g.cast(counter, &[Target::Object(spell)]);
    assert!(g.engine.state.extra_turns.is_empty());
    assert!(g.engine.state.delayed.is_empty());
}
