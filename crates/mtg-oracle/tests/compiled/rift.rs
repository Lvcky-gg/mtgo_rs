//! "Exile ~ with three time counters on it." — a suspend card's spell that suspends itself
//! as it resolves (CR 702.62b).

use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::actions::Action;

#[test]
fn the_spell_ends_up_suspended_and_ticks_down() {
    let mut table = Table::default();
    let rift = table.card(
        "{1}{U}",
        "Sorcery",
        None,
        "Draw two cards. Exile ~ with three time counters on it.\nSuspend 3—{1}{U}",
    );
    let mut game = Game::new(table);
    game.lands(2);
    let rift = game.put(rift, P0, Zone::Hand);
    game.main();
    let hand = game.count(Zone::Hand, P0);
    game.act(Action::Cast { object: rift }, &[], &[]);
    assert_eq!(game.count(Zone::Hand, P0), hand + 1, "cast (-1), drew two");
    assert_eq!(
        game.count(Zone::Graveyard, P0),
        0,
        "exiled, not put into the graveyard"
    );
    let exiled = game.engine.state.objects_in(ZoneRef::shared(Zone::Exile));
    assert_eq!(exiled.len(), 1);
    let counters = |g: &Game| -> i32 {
        g.engine
            .state
            .objects_in(ZoneRef::shared(Zone::Exile))
            .first()
            .map(|id| g.engine.state.objects[id].counters.values().sum())
            .unwrap_or(-1)
    };
    assert_eq!(counters(&game), 3);
    // Suspended: a time counter comes off at each of its owner's upkeeps.
    game.until(P1, mtg_core::Step::PrecombatMain);
    game.until(P0, mtg_core::Step::PrecombatMain);
    assert_eq!(counters(&game), 2);
}
