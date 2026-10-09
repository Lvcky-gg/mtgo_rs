//! Endurance: "When this creature enters, up to one target player puts all the cards from
//! their graveyard on the bottom of their library in a random order. Evoke—Exile a green
//! card from your hand."

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const ENDURANCE: &str = "Flash\nReach\nWhen this creature enters, up to one target player puts \
                         all the cards from their graveyard on the bottom of their library in a \
                         random order.\nEvoke—Exile a green card from your hand.";

#[test]
fn evoked_by_exiling_a_green_card_it_recycles_a_graveyard() {
    let mut table = Table::default();
    let endurance = table.card(
        "{1}{G}{G}",
        "Creature — Elemental Incarnation",
        Some((3, 4)),
        ENDURANCE,
    );
    let green = table.card("{G}", "Instant", None, "You gain 1 life.");
    let red = table.card("{R}", "Instant", None, "You gain 1 life.");
    let filler = table.bear();
    let mut game = Game::new(table);
    let endurance = game.put(endurance, P0, Zone::Hand);
    let green = game.put(green, P0, Zone::Hand);
    let red = game.put(red, P0, Zone::Hand);
    for _ in 0..3 {
        game.put(filler, P1, Zone::Graveyard);
    }
    let actions = game.main();
    let evoke = actions
        .iter()
        .find(|a| matches!(a, Action::CastAlternative { object, .. } if *object == endurance))
        .expect("evoke offered without mana")
        .clone();
    let library = game.count(Zone::Library, P1);
    let c = game.pending.take().unwrap();
    game.engine
        .answer(&game.table, c.id, Answer::Action(evoke))
        .unwrap();
    let mut offered = Vec::new();
    game.drive(
        |_, c| match &c.kind {
            ChoiceKind::ChooseObjects { from, .. } => {
                offered = from.clone();
                Some(Answer::Objects(vec![green]))
            }
            ChoiceKind::ChooseTargets { .. } => {
                Some(Answer::Targets(vec![vec![Target::Player(P1)]]))
            }
            ChoiceKind::OrderTriggers { triggers, .. } => Some(Answer::Order(triggers.clone())),
            _ => None,
        },
        |game| game.stack().is_empty() && game.engine.state.priority == Some(P0),
    );
    // The drawn filler Bear is green too; the red card and Endurance itself are not
    // offered.
    assert!(offered.contains(&green));
    assert!(!offered.contains(&red) && !offered.contains(&endurance));
    assert_eq!(game.count(Zone::Graveyard, P1), 0);
    assert_eq!(game.count(Zone::Library, P1), library + 3);
    assert!(
        game.engine
            .state
            .objects_in(mtg_core::ZoneRef::shared(Zone::Exile))
            .len()
            == 1,
        "the green card was exiled"
    );
    assert_eq!(game.count(Zone::Graveyard, P0), 1, "evoked: sacrificed");
}
