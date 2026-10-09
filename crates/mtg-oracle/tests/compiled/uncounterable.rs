//! "Spells you control can't be countered." (Hexing Squelcher): a static ability of a
//! permanent reaching spells on the stack.

use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

/// P0 casts a sorcery; P1 answers with a counterspell. Returns whether P0 gained the
/// sorcery's life.
fn countered(squelcher_text: &str, squelcher_owner: mtg_core::PlayerId) -> bool {
    let mut table = Table::default();
    let squelcher = table.card("{1}{R}", "Creature — Goblin", Some((2, 2)), squelcher_text);
    let sorcery = table.card("{0}", "Sorcery", None, "You gain 3 life.");
    let counter = table.card("{0}", "Instant", None, "Counter target spell.");
    let mut game = Game::new(table);
    game.put(squelcher, squelcher_owner, Zone::Battlefield);
    let sorcery = game.put(sorcery, P0, Zone::Hand);
    let counter = game.put(counter, P1, Zone::Hand);
    game.main();
    let c = game.pending.take().unwrap();
    game.engine
        .answer(
            &game.table,
            c.id,
            Answer::Action(Action::Cast { object: sorcery }),
        )
        .unwrap();
    let answered = std::cell::Cell::new(false);
    game.drive(
        |game, c| match &c.kind {
            ChoiceKind::Priority { .. }
                if c.who == P1 && !answered.get() && !game.stack().is_empty() =>
            {
                answered.set(true);
                Some(Answer::Action(Action::Cast { object: counter }))
            }
            // The only legal target: P0's sorcery.
            ChoiceKind::ChooseTargets { slots, .. } if c.who == P1 => {
                assert_eq!(slots[0].len(), 1);
                Some(Answer::Targets(vec![vec![slots[0][0]]]))
            }
            _ => None,
        },
        |game| {
            answered.get()
                && game.stack().is_empty()
                && game.engine.state.step == Step::PrecombatMain
        },
    );
    game.life(P0) != 23
}

#[test]
fn your_spells_cant_be_countered() {
    assert!(!countered("Spells you control can't be countered.", P0));
}

#[test]
fn without_it_the_spell_is_countered() {
    assert!(countered("", P0));
}

#[test]
fn an_opponents_squelcher_does_not_protect_your_spells() {
    assert!(countered("Spells you control can't be countered.", P1));
}
