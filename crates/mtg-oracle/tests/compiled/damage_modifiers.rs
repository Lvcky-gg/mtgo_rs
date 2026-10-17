//! "If a source you control would deal damage …, it deals double/triple/plus N instead."

use super::harness::*;
use mtg_core::{Target, Zone};

const FIERY: &str = "If a source you control would deal damage to a permanent or player, it deals triple that damage to that permanent or player instead.";
const TORBRAN: &str = "If a red source you control would deal damage to an opponent or a permanent an opponent controls, it deals that much damage plus 2 instead.";
const SOLPHIM: &str = "If a source you control would deal noncombat damage to an opponent or a permanent an opponent controls, it deals double that damage to that player or permanent instead.";

fn shock_player(statics: &[&str], cost: &str) -> i32 {
    let mut t = Table::default();
    let shock = t.card(cost, "Instant", None, "~ deals 2 damage to any target.");
    let ids: Vec<_> = statics
        .iter()
        .map(|s| t.card("{0}", "Enchantment", None, s))
        .collect();
    let mut g = Game::new(t);
    g.lands(1);
    for id in ids {
        g.put(id, P0, Zone::Battlefield);
    }
    let s = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Player(P1)]);
    20 - g.life(P1)
}

#[test]
fn triple_damage() {
    assert_eq!(shock_player(&[], "{R}"), 2);
    assert_eq!(shock_player(&[FIERY], "{R}"), 6);
}

#[test]
fn plus_two_only_from_red_sources() {
    assert_eq!(shock_player(&[TORBRAN], "{R}"), 4);
    assert_eq!(shock_player(&[TORBRAN], "{1}"), 2);
}

#[test]
fn doubling_before_adding() {
    // 2 × 3 + 2, the damaged player's better order.
    assert_eq!(shock_player(&[TORBRAN, FIERY], "{R}"), 8);
}

#[test]
fn torbran_spares_your_own_creatures() {
    let mut t = Table::default();
    let shock = t.card("{R}", "Instant", None, "~ deals 2 damage to any target.");
    let torbran = t.card("{0}", "Enchantment", None, TORBRAN);
    let wall = t.card("{2}", "Creature — Wall", Some((0, 3)), "");
    let mut g = Game::new(t);
    g.lands(1);
    g.put(torbran, P0, Zone::Battlefield);
    let w = g.put(wall, P0, Zone::Battlefield);
    let s = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(w)]);
    assert!(g.engine.state.objects.contains_key(&w), "2 damage, not 4");
}

#[test]
fn solphim_doubles_only_noncombat_damage() {
    assert_eq!(shock_player(&[SOLPHIM], "{R}"), 4);
    let mut t = Table::default();
    let solphim = t.card("{0}", "Enchantment", None, SOLPHIM);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(solphim, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.combat(&[b], &[], &[], &[]);
    assert_eq!(g.life(P1), 18);
}

#[test]
fn dictate_doubles_everyones_damage() {
    // Cast by P0 with P1 controlling the Dictate: still doubled.
    let mut t = Table::default();
    let shock = t.card("{R}", "Instant", None, "~ deals 2 damage to any target.");
    let dictate = t.card(
        "{0}",
        "Enchantment",
        None,
        "If a source would deal damage to a permanent or player, it deals double that damage to that permanent or player instead.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    g.put(dictate, P1, Zone::Battlefield);
    let s = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 16);
}

#[test]
fn gratuitous_violence_doubles_only_creatures() {
    let violence = "If a creature you control would deal damage to a permanent or player, it deals double that damage instead.";
    assert_eq!(shock_player(&[violence], "{R}"), 2);
    let mut t = Table::default();
    let v = t.card("{0}", "Enchantment", None, violence);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(v, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.combat(&[b], &[], &[], &[]);
    assert_eq!(g.life(P1), 16);
}
