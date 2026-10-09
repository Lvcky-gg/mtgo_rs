//! Teferi, Time Raveler: "Each opponent can cast spells only any time they could cast a
//! sorcery." and "+1: Until your next turn, you may cast sorcery spells as though they had
//! flash."

use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::{actions::Action, choice::ChoiceKind};

const TEFERI: &str = "Each opponent can cast spells only any time they could cast a sorcery.\n\
                      +1: Until your next turn, you may cast sorcery spells as though they had \
                      flash.";

/// A planeswalker put straight onto the battlefield needs its loyalty counters.
fn loyal(game: &mut Game, walker: mtg_core::ObjectId) {
    game.engine
        .state
        .objects
        .get_mut(&walker)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::Loyalty, 4);
}

#[test]
fn opponents_cast_only_at_sorcery_speed() {
    let mut table = Table::default();
    let teferi = table.loyalty("{1}{W}{U}", "Legendary Planeswalker — Teferi", 4, TEFERI);
    let instant = table.card("{0}", "Instant", None, "You gain 1 life.");
    let mut game = Game::new(table);
    let teferi = game.put(teferi, P0, Zone::Battlefield);
    loyal(&mut game, teferi);
    let instant = game.put(instant, P1, Zone::Hand);
    game.main();
    let mut in_p0s_turn = None;
    game.drive(
        |game, c| {
            if let ChoiceKind::Priority { legal } = &c.kind
                && c.who == P1
                && game.engine.state.active_player == P0
            {
                in_p0s_turn.get_or_insert_with(|| legal.clone());
            }
            None
        },
        |game| {
            game.engine.state.active_player == P1
                && game.engine.state.step == Step::PrecombatMain
                && game.stack().is_empty()
        },
    );
    let cast = Action::Cast { object: instant };
    assert!(!in_p0s_turn.unwrap().actions.contains(&cast));
    let own_main = match &game.pending.as_ref().unwrap().kind {
        ChoiceKind::Priority { legal } => legal.actions.clone(),
        _ => unreachable!(),
    };
    assert!(own_main.contains(&cast), "in their own main phase they may");
}

#[test]
fn plus_one_lets_you_cast_sorceries_at_instant_speed_until_your_next_turn() {
    let mut table = Table::default();
    let teferi = table.loyalty("{1}{W}{U}", "Legendary Planeswalker — Teferi", 4, TEFERI);
    let sorcery = table.card("{0}", "Sorcery", None, "You gain 1 life.");
    let mut game = Game::new(table);
    let teferi = game.put(teferi, P0, Zone::Battlefield);
    loyal(&mut game, teferi);
    let sorcery = game.put(sorcery, P0, Zone::Hand);
    game.main();
    let plus = mtg_engine::abilities::current(&game.engine.state, &game.table, teferi)
        .iter()
        .find(|a| {
            matches!(
                a.kind,
                mtg_ir::AbilityKind::Activated {
                    is_loyalty_ability: true,
                    ..
                }
            )
        })
        .unwrap()
        .id;
    game.act(
        Action::ActivateAbility {
            source: teferi,
            ability: plus,
        },
        &[],
        &[],
    );
    // In P1's turn, P0 gets priority in P1's upkeep.
    let mut offered = None;
    game.drive(
        |game, c| {
            if let ChoiceKind::Priority { legal } = &c.kind
                && c.who == P0
                && game.engine.state.active_player == P1
            {
                offered.get_or_insert_with(|| legal.actions.clone());
            }
            None
        },
        |game| game.engine.state.active_player == P1 && game.engine.state.step == Step::End,
    );
    assert!(offered.unwrap().contains(&Action::Cast { object: sorcery }));
}
