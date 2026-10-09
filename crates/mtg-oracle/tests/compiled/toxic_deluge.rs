//! Toxic Deluge: "As an additional cost to cast this spell, pay X life. All creatures get
//! -X/-X until end of turn." — X announced, at most the caster's life (CR 119.4).

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{actions::Action, choice::Answer};

const DELUGE: &str = "As an additional cost to cast this spell, pay X life.\nAll creatures get -X/-X until end of turn.";

#[test]
fn pays_x_life_and_shrinks_everything_by_x() {
    let mut table = Table::default();
    let deluge = table.card("{2}{B}", "Sorcery", None, DELUGE);
    let small = table.card("{2}", "Creature — Ox", Some((2, 3)), "");
    let big = table.card("{4}", "Creature — Wurm", Some((5, 5)), "");
    let mut game = Game::new(table);
    game.lands(3);
    let deluge = game.put(deluge, P0, Zone::Hand);
    let small = game.put(small, P1, Zone::Battlefield);
    let big = game.put(big, P1, Zone::Battlefield);
    game.main();
    game.act(Action::Cast { object: deluge }, &[], &[Answer::Number(3)]);
    assert_eq!(game.life(P0), 17);
    assert!(!game.engine.state.objects.contains_key(&small));
    assert_eq!(game.pt(big), (2, 2));
}

#[test]
fn x_is_capped_by_life() {
    let mut table = Table::default();
    let deluge = table.card("{2}{B}", "Sorcery", None, DELUGE);
    let mut game = Game::new(table);
    game.lands(3);
    game.engine.state.players.get_mut(&P0).unwrap().life = 4;
    let deluge = game.put(deluge, P0, Zone::Hand);
    game.main();
    let c = game.pending.take().unwrap();
    game.engine
        .answer(
            &game.table,
            c.id,
            Answer::Action(Action::Cast { object: deluge }),
        )
        .unwrap();
    let choice = loop {
        match game.engine.advance(&game.table) {
            mtg_engine::Progress::NeedsChoice(c) => break c,
            mtg_engine::Progress::Continue => {}
            other => panic!("{other:?}"),
        }
    };
    assert!(
        matches!(
            choice.kind,
            mtg_engine::choice::ChoiceKind::ChooseX { max: 4, .. }
        ),
        "{:?}",
        choice.kind
    );
}
