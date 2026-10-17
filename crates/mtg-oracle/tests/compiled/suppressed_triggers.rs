//! "Creatures entering don't cause abilities to trigger." (Torpor Orb), "Creatures entering or
//! dying …" (Hushbringer)

use super::harness::*;
use mtg_core::{Target, Zone};

const ORB: &str = "Creatures entering don't cause abilities to trigger.";
const HUSH: &str = "Creatures entering or dying don't cause abilities to trigger.";

fn life_after_cast(static_text: Option<&str>, type_line: &str, pt: Option<(i32, i32)>) -> i32 {
    let mut t = Table::default();
    let orb = t.card("{2}", "Artifact", None, static_text.unwrap_or(""));
    let body = t.card(
        "{0}",
        type_line,
        pt,
        "When this permanent enters, you gain 1 life.",
    );
    let mut g = Game::new(t);
    if static_text.is_some() {
        g.put(orb, P0, Zone::Battlefield);
    }
    let s = g.put(body, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    g.life(P0)
}

#[test]
fn torpor_orb_stops_creatures_entering_but_not_artifacts() {
    assert_eq!(life_after_cast(None, "Creature — Cleric", Some((1, 1))), 21);
    assert_eq!(
        life_after_cast(Some(ORB), "Creature — Cleric", Some((1, 1))),
        20
    );
    assert_eq!(life_after_cast(Some(ORB), "Artifact", None), 21);
}

#[test]
fn hushbringer_also_stops_dies_triggers() {
    let mut t = Table::default();
    let hush = t.card(
        "{1}{W}",
        "Artifact Creature — Phyrexian Gremlin",
        Some((1, 2)),
        HUSH,
    );
    let mourner = t.card(
        "{0}",
        "Creature — Cleric",
        Some((1, 1)),
        "When this creature dies, you gain 3 life.",
    );
    let shock = t.card(
        "{0}",
        "Instant",
        None,
        "~ deals 2 damage to target creature.",
    );
    let mut g = Game::new(t);
    g.put(hush, P0, Zone::Battlefield);
    let victim = g.put(mourner, P0, Zone::Battlefield);
    let s = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(victim)]);
    assert!(!g.engine.state.objects.contains_key(&victim));
    assert_eq!(g.life(P0), 20);
}

#[test]
fn elesh_norn_doubles_yours_and_stops_theirs() {
    let mut t = Table::default();
    let norn = t.card(
        "{4}{W}",
        "Legendary Creature — Phyrexian Praetor",
        Some((4, 7)),
        "Vigilance\nIf a permanent entering causes a triggered ability of a permanent you control to trigger, that ability triggers an additional time.\nPermanents entering don't cause abilities of permanents your opponents control to trigger.",
    );
    let body = t.card(
        "{0}",
        "Creature — Cleric",
        Some((1, 1)),
        "When this creature enters, you gain 1 life.",
    );
    let watcher = t.card(
        "{0}",
        "Enchantment",
        None,
        "Whenever a creature enters, you gain 1 life.",
    );
    let mut g = Game::new(t);
    g.put(norn, P0, Zone::Battlefield);
    g.put(watcher, P1, Zone::Battlefield);
    let s = g.put(body, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!(g.life(P0), 22);
    assert_eq!(g.life(P1), 20);
}
