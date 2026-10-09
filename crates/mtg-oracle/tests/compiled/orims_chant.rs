//! Orim's Chant: "Target player can't cast spells this turn." — the targeted player, fixed
//! as it resolves (the effect outlives the spell and its target list).

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::{
    actions::{Action, LegalActions},
    choice::ChoiceKind,
};

const CHANT: &str = "Kicker {W}\nTarget player can't cast spells this turn. If this spell was \
                     kicked, creatures can't attack this turn.";

/// P1's priority offers later in P0's turn, and in P1's own turn.
fn offers(
    chant_target: Option<mtg_core::PlayerId>,
) -> (LegalActions, LegalActions, mtg_core::ObjectId) {
    let mut table = Table::default();
    let chant = table.card("{0}", "Instant", None, CHANT);
    let instant = table.card("{0}", "Instant", None, "You gain 1 life.");
    let mut game = Game::new(table);
    let chant = game.put(chant, P0, Zone::Hand);
    let instant = game.put(instant, P1, Zone::Hand);
    game.main();
    if let Some(who) = chant_target {
        game.act(Action::Cast { object: chant }, &[Target::Player(who)], &[]);
    }
    let mut theirs = None;
    let mut own = None;
    game.drive(
        |game, c| {
            if let ChoiceKind::Priority { legal } = &c.kind
                && c.who == P1
            {
                if game.engine.state.active_player == P0 {
                    theirs.get_or_insert_with(|| legal.clone());
                } else {
                    own.get_or_insert_with(|| legal.clone());
                }
            }
            None
        },
        |game| {
            game.engine.state.active_player == P1
                && game.engine.state.step == mtg_core::Step::PrecombatMain
        },
    );
    (theirs.unwrap(), own.unwrap(), instant)
}

fn can_cast(legal: &LegalActions, spell: mtg_core::ObjectId) -> bool {
    legal.actions.contains(&Action::Cast { object: spell })
}

#[test]
fn the_target_cant_cast_spells_this_turn() {
    let (theirs, own, instant) = offers(Some(P1));
    assert!(!can_cast(&theirs, instant));
    assert!(can_cast(&own, instant), "only this turn");
}

#[test]
fn targeting_yourself_leaves_the_opponent_free() {
    let (theirs, _, instant) = offers(Some(P0));
    assert!(can_cast(&theirs, instant));
}

#[test]
fn without_it_they_can_cast() {
    let (theirs, _, instant) = offers(None);
    assert!(can_cast(&theirs, instant));
}
