//! Hardened Scales, Branching Evolution, Doubling Season: counters put on your permanents.

use super::harness::*;
use mtg_core::{CounterKind, Target, Zone};

const SCALES: &str = "If one or more +1/+1 counters would be put on a creature you control, that many plus one +1/+1 counters are put on it instead.";
const BRANCHING: &str = "If one or more +1/+1 counters would be put on a creature you control, twice that many +1/+1 counters are put on that creature instead.";
const SEASON: &str = "If an effect would put one or more counters on a permanent you control, it puts twice that many of those counters on that permanent instead.";

fn counters_after(statics: &[&str], spell_text: &str, kind: CounterKind) -> i32 {
    let mut t = Table::default();
    let spell = t.card("{0}", "Sorcery", None, spell_text);
    let ids: Vec<_> = statics
        .iter()
        .map(|s| t.card("{0}", "Enchantment", None, s))
        .collect();
    let bear = t.bear();
    let mut g = Game::new(t);
    for id in ids {
        g.put(id, P0, Zone::Battlefield);
    }
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(b)]);
    *g.engine.state.objects[&b].counters.get(&kind).unwrap_or(&0)
}

const ONE: &str = "Put a +1/+1 counter on target creature.";

#[test]
fn hardened_scales_adds_one() {
    assert_eq!(counters_after(&[], ONE, CounterKind::PlusOnePlusOne), 1);
    assert_eq!(
        counters_after(&[SCALES], ONE, CounterKind::PlusOnePlusOne),
        2
    );
}

#[test]
fn adding_before_doubling() {
    assert_eq!(
        counters_after(&[SCALES, BRANCHING], ONE, CounterKind::PlusOnePlusOne),
        4
    );
}

#[test]
fn doubling_season_doubles_any_kind() {
    let stun = "Put two stun counters on target creature.";
    assert_eq!(counters_after(&[SEASON], stun, CounterKind::Stun), 4);
    // Scales only cares about +1/+1 counters.
    assert_eq!(counters_after(&[SCALES], stun, CounterKind::Stun), 2);
}

#[test]
fn entering_with_counters_and_proliferating() {
    let mut t = Table::default();
    let scales = t.card("{G}", "Enchantment", None, SCALES);
    let hydra = t.card(
        "{0}",
        "Creature — Hydra",
        Some((0, 0)),
        "This creature enters with two +1/+1 counters on it.",
    );
    let prolif = t.card("{0}", "Sorcery", None, "Proliferate.");
    let mut g = Game::new(t);
    g.put(scales, P0, Zone::Battlefield);
    let h = g.put(hydra, P0, Zone::Hand);
    let p = g.put(prolif, P0, Zone::Hand);
    g.main();
    g.cast(h, &[]);
    let body = g.find(hydra).unwrap();
    assert_eq!(g.pt(body), (3, 3));
    g.main();
    g.act(
        mtg_engine::actions::Action::Cast { object: p },
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![body])],
    );
    assert_eq!(g.pt(body), (5, 5));
}
