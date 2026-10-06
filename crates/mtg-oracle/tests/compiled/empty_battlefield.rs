use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::{Progress, choice::ChoiceKind};

#[test]
fn empty_battlefield_condition_counts_creatures_controlled_by_either_player() {
    for owner in [None, Some(P0), Some(P1)] {
        let mut table = Table::default();
        let enchantment = table.card("{2}{B}{B}", "Enchantment", None,
            "At the beginning of the end step, if no creatures are on the battlefield, sacrifice this enchantment.");
        let creature = table.bear();
        let mut game = Game::new(table);
        let source = game.put(enchantment, P0, Zone::Battlefield);
        if let Some(owner) = owner {
            game.put(creature, owner, Zone::Battlefield);
        }
        game.until(P0, Step::End);
        assert_eq!(
            game.engine.state.objects.contains_key(&source),
            owner.is_some()
        );
        assert_eq!(
            game.count(Zone::Graveyard, P0),
            usize::from(owner.is_none())
        );
    }
}

#[test]
fn empty_battlefield_condition_is_rechecked_after_a_creature_enters_in_response() {
    let mut table = Table::default();
    let enchantment = table.card("{2}{B}{B}", "Enchantment", None,
        "At the beginning of the end step, if no creatures are on the battlefield, sacrifice this enchantment.");
    let maker = table.card(
        "{1}",
        "Artifact",
        None,
        "{0}: Create a 1/1 white Soldier creature token.",
    );
    let mut game = Game::new(table);
    let source = game.put(enchantment, P0, Zone::Battlefield);
    let maker = game.put(maker, P0, Zone::Battlefield);
    for _ in 0..1000 {
        match game.engine.advance(&game.table) {
            Progress::NeedsChoice(choice) => {
                if matches!(choice.kind, ChoiceKind::Priority { .. })
                    && choice.who == P0
                    && game.engine.state.step == Step::End
                    && !game.stack().is_empty()
                {
                    game.pending = Some(choice);
                    game.act(activate(maker, 0), &[], &[]);
                    assert!(game.engine.state.objects.contains_key(&source));
                    assert_eq!(game.engine.state.battlefield().len(), 3);
                    return;
                }
                game.engine
                    .answer(
                        &game.table,
                        choice.id,
                        choice.default.expect("default choice"),
                    )
                    .unwrap();
            }
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended"),
        }
    }
    panic!("end-step trigger did not offer priority");
}
