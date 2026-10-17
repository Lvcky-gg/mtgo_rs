//! "If a triggered ability of a creature you control with power 2 or less triggers, that
//! ability triggers an additional time." (Delney, Streetwise Lookout)

use super::harness::*;
use mtg_core::Zone;

const DELNEY: &str = "If a triggered ability of a creature you control with power 2 or less triggers, that ability triggers an additional time.";
const GAIN: &str = "When this creature enters, you gain 1 life.";

fn life_after_casting(pt: (i32, i32), delney: bool) -> i32 {
    let mut t = Table::default();
    let lookout = t.card(
        "{2}{W}",
        "Legendary Creature — Human Scout",
        Some((2, 2)),
        DELNEY,
    );
    let body = t.card("{0}", "Creature — Cleric", Some(pt), GAIN);
    let mut g = Game::new(t);
    if delney {
        g.put(lookout, P0, Zone::Battlefield);
    }
    let s = g.put(body, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    g.life(P0)
}

#[test]
fn a_small_creatures_trigger_happens_twice() {
    assert_eq!(life_after_casting((1, 1), false), 21);
    assert_eq!(life_after_casting((1, 1), true), 22);
}

#[test]
fn a_big_creatures_trigger_happens_once() {
    assert_eq!(life_after_casting((3, 3), true), 21);
}

const NORN: &str = "If a permanent entering causes a triggered ability of a permanent you control to trigger, that ability triggers an additional time.";

#[test]
fn only_triggers_caused_by_something_entering_are_doubled() {
    let mut t = Table::default();
    let norn = t.card(
        "{2}{W}",
        "Legendary Creature — Phyrexian Praetor",
        Some((4, 7)),
        NORN,
    );
    let enters = t.card("{0}", "Creature — Cleric", Some((1, 1)), GAIN);
    let caster = t.card(
        "{0}",
        "Enchantment",
        None,
        "Whenever you cast a spell, you gain 1 life.",
    );
    let mut g = Game::new(t);
    g.put(norn, P0, Zone::Battlefield);
    g.put(caster, P0, Zone::Battlefield);
    let s = g.put(enters, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    // Casting: one life (not doubled). Entering: two.
    assert_eq!(g.life(P0), 23);
}

#[test]
fn greenwarden_doubles_landfall_only() {
    let mut t = Table::default();
    let warden = t.card(
        "{4}{G}{G}",
        "Creature — Elemental",
        Some((5, 7)),
        "If a land entering causes a triggered ability of a permanent you control to trigger, that ability triggers an additional time.",
    );
    let fall = t.card(
        "{0}",
        "Enchantment",
        None,
        "Whenever a land you control enters, you gain 1 life.\nWhenever a creature you control enters, you gain 1 life.",
    );
    let land = t.card("", "Land", None, "");
    let body = t.card("{0}", "Creature — Cleric", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.put(warden, P0, Zone::Battlefield);
    g.put(fall, P0, Zone::Battlefield);
    let l = g.put(land, P0, Zone::Hand);
    let b = g.put(body, P0, Zone::Hand);
    g.main();
    g.act(
        mtg_engine::actions::Action::PlayLand { object: l },
        &[],
        &[],
    );
    assert_eq!(g.life(P0), 22, "landfall twice");
    g.main();
    g.cast(b, &[]);
    assert_eq!(g.life(P0), 23, "a creature entering, once");
}
