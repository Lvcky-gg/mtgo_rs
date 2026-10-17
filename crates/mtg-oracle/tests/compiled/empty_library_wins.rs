//! "If you would draw a card while your library has no cards in it, you win the game
//! instead." (Laboratory Maniac)

use super::harness::*;
use mtg_core::{Zone, ZoneRef};

const MANIAC: &str =
    "If you would draw a card while your library has no cards in it, you win the game instead.";

fn draw_with_empty_library(maniac: bool) -> (bool, bool) {
    let mut t = Table::default();
    let wizard = t.card("{2}{U}", "Creature — Human Wizard", Some((2, 2)), MANIAC);
    let draw = t.card("{0}", "Instant", None, "Draw a card.");
    let mut g = Game::new(t);
    if maniac {
        g.put(wizard, P0, Zone::Battlefield);
    }
    let s = g.put(draw, P0, Zone::Hand);
    g.main();
    let library = ZoneRef::of(Zone::Library, P0);
    for id in g.engine.state.objects_in(library) {
        g.engine.state.objects.remove(&id);
    }
    g.engine.state.zone_order.insert(library, Vec::new());
    g.cast(s, &[]);
    (
        g.engine.state.player(P0).has_lost,
        g.engine.state.player(P1).has_lost,
    )
}

#[test]
fn the_maniac_wins_instead_of_drawing() {
    assert_eq!(draw_with_empty_library(true), (false, true));
}

#[test]
fn without_it_nobody_has_lost_yet() {
    // The draw is only recorded; state-based actions end the game for P0 later.
    assert!(!draw_with_empty_library(false).1);
}
