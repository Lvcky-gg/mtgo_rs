//! "… you may …. Do this only once each turn." — declining doesn't use it up.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{actions::Action, choice::Answer};

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
