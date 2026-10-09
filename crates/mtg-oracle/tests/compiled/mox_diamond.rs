//! Mox Diamond: "If this artifact would enter, you may discard a land card instead. If you
//! do, put this artifact onto the battlefield. If you don't, put it into its owner's
//! graveyard." — decided before it enters (CR 614.12).

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const DIAMOND: &str = "If this artifact would enter, you may discard a land card instead. If \
                       you do, put this artifact onto the battlefield. If you don't, put it \
                       into its owner's graveyard.\n{T}: Add one mana of any color.";

/// Cast the Mox; `discard` answers its question (`None`: none asked). Whether it ended up
/// on the battlefield, and whether a question was asked.
fn diamond(land_in_hand: bool, discard: bool) -> (bool, bool) {
    let mut table = Table::default();
    let mox = table.card("{0}", "Artifact", None, DIAMOND);
    let land = table.card("", "Land", None, "");
    let mut game = Game::new(table);
    let mox = game.put(mox, P0, Zone::Hand);
    let land = land_in_hand.then(|| game.put(land, P0, Zone::Hand));
    game.main();
    let c = game.pending.take().unwrap();
    game.engine
        .answer(
            &game.table,
            c.id,
            Answer::Action(Action::Cast { object: mox }),
        )
        .unwrap();
    let asked = std::cell::Cell::new(false);
    game.drive(
        |_, c| match &c.kind {
            ChoiceKind::ChooseObjects { from, .. } => {
                asked.set(true);
                assert_eq!(from, &vec![land.unwrap()]);
                Some(Answer::Objects(if discard { from.clone() } else { vec![] }))
            }
            _ => None,
        },
        |game| game.stack().is_empty() && game.engine.state.priority == Some(P0),
    );
    let on_battlefield = game
        .engine
        .state
        .battlefield()
        .iter()
        .any(|id| game.engine.state.objects[id].card == mtg_core::CardId(0));
    if let Some(land) = land {
        assert_eq!(
            !game.engine.state.objects.contains_key(&land),
            discard,
            "the land is discarded only if chosen"
        );
    }
    (on_battlefield, asked.get())
}

#[test]
fn discarding_a_land_puts_it_onto_the_battlefield() {
    assert_eq!(diamond(true, true), (true, true));
}

#[test]
fn declining_puts_it_into_the_graveyard() {
    assert_eq!(diamond(true, false), (false, true));
}

#[test]
fn without_a_land_it_goes_to_the_graveyard_unasked() {
    assert_eq!(diamond(false, false), (false, false));
}
