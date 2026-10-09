//! "Choose one. If you control a commander as you cast this spell, you may choose both
//! instead." (Jeska's Will) — how many modes is decided as the spell is cast.

use super::harness::*;
use mtg_core::{Color, Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const JESKA: &str = "Choose one. If you control a commander as you cast this spell, you may \
                     choose both instead.\n• Add {R} for each card in target opponent's hand.\n\
                     • Exile the top three cards of your library. You may play them this turn.";

/// Cast Jeska's Will, choosing as many modes as offered; how many that was, and P0's red
/// mana afterwards.
fn jeska(commander: bool) -> (u8, u16) {
    let mut table = Table::default();
    let will = table.card("{0}", "Sorcery", None, JESKA);
    let general = table.card("{0}", "Legendary Creature — Human", Some((1, 1)), "");
    let filler = table.bear();
    let mut game = Game::new(table);
    if commander {
        game.engine.state.commander.commanders.insert(P0, general);
        game.put(general, P0, Zone::Battlefield);
    }
    for _ in 0..3 {
        game.put(filler, P1, Zone::Hand);
    }
    let will = game.put(will, P0, Zone::Hand);
    game.main();
    let c = game.pending.take().unwrap();
    game.engine
        .answer(
            &game.table,
            c.id,
            Answer::Action(Action::Cast { object: will }),
        )
        .unwrap();
    let mut offered = 0;
    game.drive(
        |_, c| match &c.kind {
            ChoiceKind::ChooseModes { count, .. } => {
                offered = *count;
                Some(Answer::Modes((0..*count).collect()))
            }
            ChoiceKind::ChooseTargets { .. } => {
                Some(Answer::Targets(vec![vec![Target::Player(P1)]]))
            }
            _ => None,
        },
        |game| game.stack().is_empty() && game.engine.state.priority == Some(P0),
    );
    (
        offered,
        game.engine.state.player(P0).mana.amounts[Color::Red as usize],
    )
}

#[test]
fn with_a_commander_both_modes() {
    let (offered, red) = jeska(true);
    assert_eq!(offered, 2);
    assert_eq!(red, 3, "one {{R}} for each card in the opponent's hand");
}

#[test]
fn without_one_mode() {
    let (offered, _) = jeska(false);
    assert_eq!(offered, 1);
}
