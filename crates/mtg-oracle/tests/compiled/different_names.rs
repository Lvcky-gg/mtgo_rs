//! "Search your library for up to two creature cards with different names, reveal them, put
//! them into your hand, then shuffle." (Shared Summons): no two found share a name.

use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

const SUMMONS: &str = "Search your library for up to two creature cards with different names, reveal them, put them into your hand, then shuffle.";

fn library(g: &Game) -> Vec<mtg_core::ObjectId> {
    g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))
}

#[test]
fn two_different_names_are_found() {
    let mut t = Table::default();
    let summons = t.card("{0}", "Instant", None, SUMMONS);
    let elf = t.named_card("Elf", "{G}", "Creature — Elf", Some((1, 1)), "");
    let mut g = Game::new(t);
    let spell = g.put(summons, P0, Zone::Hand);
    let elf = g.put(elf, P0, Zone::Library);
    g.main();
    let bear = library(&g)[0];
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![bear]), Answer::Objects(vec![elf])],
    );
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand + 1,
        "the spell left, two came"
    );
}

#[test]
fn a_second_card_of_the_same_name_is_not_offered() {
    let mut t = Table::default();
    let summons = t.card("{0}", "Instant", None, SUMMONS);
    let mut g = Game::new(t);
    let spell = g.put(summons, P0, Zone::Hand);
    g.main();
    // The library is all the same bear.
    let lib = library(&g);
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![lib[0]]), Answer::Objects(vec![lib[1]])],
    );
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand,
        "the spell left, one bear came"
    );
}
