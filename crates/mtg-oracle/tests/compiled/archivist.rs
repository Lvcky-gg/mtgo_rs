//! Archivist of Oghma: "Whenever an opponent searches their library, you gain 1 life and
//! draw a card." — searching is the event, whatever is found (CR 701.19).

use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const ARCHIVIST: &str =
    "Whenever an opponent searches their library, you gain 1 life and draw a card.";
const TUTOR: &str = "Search your library for a card, put it into your hand, then shuffle.";

#[test]
fn an_opponents_search_triggers_it() {
    let mut table = Table::default();
    let archivist = table.card("{1}{W}", "Creature — Human Cleric", Some((1, 2)), ARCHIVIST);
    let tutor = table.card("{0}", "Sorcery", None, TUTOR);
    let mut game = Game::new(table);
    game.put(archivist, P0, Zone::Battlefield);
    let tutor = game.put(tutor, P1, Zone::Hand);
    game.main();
    let hand = game.count(Zone::Hand, P0);
    let cast = std::cell::Cell::new(false);
    game.drive(
        |game, c| match &c.kind {
            ChoiceKind::Priority { .. }
                if c.who == P1
                    && !cast.get()
                    && game.engine.state.active_player == P1
                    && game.engine.state.step == Step::PrecombatMain =>
            {
                cast.set(true);
                Some(Answer::Action(Action::Cast { object: tutor }))
            }
            _ => None,
        },
        |game| cast.get() && game.stack().is_empty(),
    );
    assert_eq!(game.life(P0), 21);
    assert_eq!(game.count(Zone::Hand, P0), hand + 1);
}

#[test]
fn your_own_search_does_not() {
    let mut table = Table::default();
    let archivist = table.card("{1}{W}", "Creature — Human Cleric", Some((1, 2)), ARCHIVIST);
    let tutor = table.card("{0}", "Sorcery", None, TUTOR);
    let mut game = Game::new(table);
    game.put(archivist, P0, Zone::Battlefield);
    let tutor = game.put(tutor, P0, Zone::Hand);
    game.main();
    game.cast(tutor, &[]);
    assert_eq!(game.life(P0), 20);
}
