//! "… you may …. Do this only once each turn." — declining doesn't use it up.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn a_draw_batch_and_a_declined_optional_effect_still_use_one_trigger() {
    let mut table = Table::default();
    let scholar = table.card(
        "{0}",
        "Enchantment",
        None,
        "Whenever an opponent draws a card, you may gain 1 life. This ability triggers only once each turn.",
    );
    let draw = table.card("{0}", "Instant", None, "Target player draws two cards.");
    let mut game = Game::new(table);
    game.put(scholar, P0, Zone::Battlefield);
    let first = game.put(draw, P0, Zone::Hand);
    let second = game.put(draw, P0, Zone::Hand);
    let third = game.put(draw, P0, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: first },
        &[mtg_core::Target::Player(P1)],
        &[Answer::Bool(false)],
    );
    assert_eq!(game.life(P0), 20);
    game.act(
        Action::Cast { object: second },
        &[mtg_core::Target::Player(P1)],
        &[Answer::Bool(true)],
    );
    assert_eq!(
        game.life(P0),
        20,
        "declining the effect still consumes the trigger limit"
    );
    game.until(P1, mtg_core::Step::PrecombatMain);
    game.main();
    game.act(
        Action::Cast { object: third },
        &[mtg_core::Target::Player(P1)],
        &[Answer::Bool(true)],
    );
    assert_eq!(
        game.life(P0),
        21,
        "a later turn resets the limit, including within a draw batch"
    );
}

#[test]
fn only_doing_it_counts_and_it_resets_each_turn() {
    let mut table = Table::default();
    let scholar = table.card(
        "{1}{W}",
        "Enchantment",
        None,
        "Whenever you gain life, you may draw a card. Do this only once each turn.",
    );
    let heal = table.card("{W}", "Instant", None, "You gain 1 life.");
    let mut game = Game::new(table);
    game.lands(4);
    game.put(scholar, P0, Zone::Battlefield);
    let heals: Vec<_> = (0..4).map(|_| game.put(heal, P0, Zone::Hand)).collect();
    game.main();
    let before = game.count(Zone::Hand, P0);
    game.act(
        Action::Cast { object: heals[0] },
        &[],
        &[Answer::Bool(false)],
    );
    assert_eq!(game.count(Zone::Hand, P0), before - 1, "declined: no card");
    game.act(
        Action::Cast { object: heals[1] },
        &[],
        &[Answer::Bool(true)],
    );
    assert_eq!(game.count(Zone::Hand, P0), before - 1, "cast one, drew one");
    game.act(
        Action::Cast { object: heals[2] },
        &[],
        &[Answer::Bool(true)],
    );
    assert_eq!(
        game.count(Zone::Hand, P0),
        before - 2,
        "already done this turn"
    );
    // The next turn it works again.
    game.until(P1, mtg_core::Step::PrecombatMain);
    game.main();
    let before = game.count(Zone::Hand, P0);
    game.act(
        Action::Cast { object: heals[3] },
        &[],
        &[Answer::Bool(true)],
    );
    assert_eq!(game.count(Zone::Hand, P0), before);
}
