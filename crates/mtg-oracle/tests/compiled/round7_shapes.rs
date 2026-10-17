//! Suture Priest's "that player", attacking tokens, "becomes an Angel", and counting Plains.

use super::harness::*;
use mtg_core::{Keyword, Step, Target, Zone};

#[test]
fn suture_priest_drains_the_controller_of_what_entered() {
    let mut t = Table::default();
    let priest = t.card(
        "{1}{W}",
        "Creature — Cleric",
        Some((1, 1)),
        "Whenever a creature an opponent controls enters, that player loses 1 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(priest, P1, Zone::Battlefield);
    g.lands(2);
    let s = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    // The same binding as Suture Priest's "you may have that player lose 1 life".
    assert_eq!(g.life(P0), 19);
}

#[test]
fn tokens_that_are_attacking_are_untapped_attackers() {
    let mut t = Table::default();
    let ship = t.card(
        "{0}",
        "Creature — Ship",
        Some((5, 5)),
        "Whenever this creature attacks, create two 4/4 white Angel creature tokens with flying and vigilance that are attacking.",
    );
    let mut g = Game::new(t);
    let s = g.put(ship, P0, Zone::Battlefield);
    g.main();
    g.combat(&[s], &[], &[], &[]);
    // Ship 5 + two Angels 4 + 4.
    assert_eq!(g.life(P1), 7);
    let angels: Vec<_> = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].card.0 >= mtg_ir::walk::TOKEN_CARD_BASE)
        .collect();
    assert_eq!(angels.len(), 2);
    assert!(angels.iter().all(|a| !g.engine.state.objects[a].tapped));
}

#[test]
fn it_becomes_an_angel_and_gains_flying_for_good() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Put two +1/+1 counters on target creature. It becomes an Angel in addition to its other types and gains flying.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(b)]);
    assert_eq!(g.pt(b), (4, 4));
    assert!(g.has(b, Keyword::Flying));
    g.pending = None;
    g.until(P0, Step::PrecombatMain);
    assert!(g.has(b, Keyword::Flying), "still flying next turn");
}

#[test]
fn seven_plains_counts_the_plains() {
    for n in [6, 7] {
        let mut t2 = Table::default();
        let plains2 = t2.card("", "Basic Land — Plains", None, "");
        let spell2 = t2.card(
            "{0}",
            "Sorcery",
            None,
            "If you control seven or more Plains, draw a card.",
        );
        let mut g = Game::new(t2);
        for _ in 0..n {
            g.put(plains2, P0, Zone::Battlefield);
        }
        let s = g.put(spell2, P0, Zone::Hand);
        g.main();
        let hand = g.count(Zone::Hand, P0);
        g.cast(s, &[]);
        assert_eq!(
            g.count(Zone::Hand, P0),
            hand - 1 + usize::from(n >= 7),
            "{n} Plains"
        );
    }
}

#[test]
fn massacre_wurm_drains_the_dead_creatures_controller() {
    let mut t = Table::default();
    let wurm = t.card(
        "{3}{B}{B}{B}",
        "Creature — Phyrexian Wurm",
        Some((6, 5)),
        "Whenever a creature an opponent controls dies, that player loses 2 life.",
    );
    let shock = t.card(
        "{0}",
        "Instant",
        None,
        "~ deals 2 damage to target creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(wurm, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(b)]);
    assert_eq!(g.life(P1), 18);
}

#[test]
fn exquisite_blood_gains_what_an_opponent_loses() {
    let mut t = Table::default();
    let blood = t.card(
        "{4}{B}",
        "Enchantment",
        None,
        "Whenever an opponent loses life, you gain that much life.",
    );
    let drain = t.card("{0}", "Sorcery", None, "Each opponent loses 3 life.");
    let mut g = Game::new(t);
    g.put(blood, P0, Zone::Battlefield);
    let s = g.put(drain, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!((g.life(P0), g.life(P1)), (23, 17));
}

#[test]
fn grave_pact_makes_each_other_player_sacrifice() {
    let mut t = Table::default();
    let pact = t.card(
        "{1}{B}{B}{B}",
        "Enchantment",
        None,
        "Whenever a creature you control dies, each other player sacrifices a creature of their choice.",
    );
    let shock = t.card(
        "{0}",
        "Instant",
        None,
        "~ deals 2 damage to target creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(pact, P0, Zone::Battlefield);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(mine)]);
    assert!(!g.engine.state.objects.contains_key(&theirs));
}
