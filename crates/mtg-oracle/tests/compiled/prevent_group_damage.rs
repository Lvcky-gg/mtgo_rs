//! CR 615 — "Prevent all damage that would be dealt to creatures this turn." (Holy Day
//! style), "… by creatures this turn", "… to you and creatures you control this turn"
//! (Safe Passage): a group, including what arrives later (CR 611.2c). Also "you and Humans
//! you control have hexproof" (Sigarda) — the player, not a "You" subtype.

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

const BOLT: &str = "~ deals 3 damage to any target.";

#[test]
fn creatures_are_shielded_even_one_that_arrives_later() {
    let mut t = Table::default();
    let fog = t.card(
        "{W}",
        "Instant",
        None,
        "Prevent all damage that would be dealt to creatures this turn.",
    );
    let bolt = t.card("{0}", "Instant", None, BOLT);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let fog = g.put(fog, P0, Zone::Hand);
    let bolt = g.put(bolt, P0, Zone::Hand);
    let late = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(fog, &[]);
    g.cast(late, &[]);
    let late = g.find(bear).unwrap();
    g.cast(bolt, &[Target::Object(late)]);
    assert!(g.find(bear).is_some(), "still on the battlefield");
    assert_eq!(g.pt(late), (2, 2));
}

#[test]
fn a_player_is_not_a_creature() {
    let mut t = Table::default();
    let fog = t.card(
        "{W}",
        "Instant",
        None,
        "Prevent all damage that would be dealt to creatures this turn.",
    );
    let bolt = t.card("{0}", "Instant", None, BOLT);
    let mut g = Game::new(t);
    g.lands(1);
    let fog = g.put(fog, P0, Zone::Hand);
    let bolt = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(fog, &[]);
    let before = g.life(P1);
    g.cast(bolt, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), before - 3);
}

#[test]
fn damage_by_creatures_is_prevented() {
    let mut t = Table::default();
    let fog = t.card(
        "{G}",
        "Instant",
        None,
        "Prevent all damage that would be dealt by creatures this turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let fog = g.put(fog, P0, Zone::Hand);
    let bear = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.cast(fog, &[]);
    let before = g.life(P1);
    g.combat(&[bear], &[], &[], &[]);
    assert_eq!(g.life(P1), before);
}

#[test]
fn only_opponents_creatures_are_stopped() {
    let mut t = Table::default();
    let fog = t.card(
        "{W}",
        "Instant",
        None,
        "Prevent all damage that would be dealt this turn by creatures your opponents control.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let fog = g.put(fog, P0, Zone::Hand);
    let bear = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.cast(fog, &[]);
    let before = g.life(P1);
    g.combat(&[bear], &[], &[], &[]);
    assert_eq!(
        g.life(P1),
        before - 2,
        "your own creature still deals damage"
    );
}

#[test]
fn safe_passage_shields_you_too() {
    let mut t = Table::default();
    let passage = t.card(
        "{2}{W}",
        "Instant",
        None,
        "Prevent all damage that would be dealt this turn to you and creatures you control.",
    );
    let bolt = t.card("{0}", "Instant", None, BOLT);
    let mut g = Game::new(t);
    g.lands(3);
    let passage = g.put(passage, P0, Zone::Hand);
    let bolt = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(passage, &[]);
    let before = g.life(P0);
    g.cast(bolt, &[Target::Player(P0)]);
    assert_eq!(g.life(P0), before);
}

#[test]
fn sigarda_gives_you_hexproof() {
    let mut t = Table::default();
    let sigarda = t.card(
        "{3}{G}{W}",
        "Legendary Creature — Angel",
        Some((4, 5)),
        "You and Elves you control have hexproof.",
    );
    let mut g = Game::new(t);
    g.put(sigarda, P0, Zone::Battlefield);
    g.main();
    let state = &g.engine.state;
    assert!(mtg_engine::targeting::player_hexproof_from(
        state, &g.table, P0, P1
    ));
    assert!(!mtg_engine::targeting::player_hexproof_from(
        state, &g.table, P1, P0
    ));
}

#[test]
fn lazotep_plating_gives_you_hexproof_after_it_resolves() {
    let mut t = Table::default();
    let plating = t.card(
        "{1}{U}",
        "Instant",
        None,
        "You and permanents you control gain hexproof until end of turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let plating = g.put(plating, P0, Zone::Hand);
    let bear = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.cast(plating, &[]);
    assert!(mtg_engine::targeting::player_hexproof_from(
        &g.engine.state,
        &g.table,
        P0,
        P1
    ));
    assert!(g.has(bear, mtg_core::Keyword::Hexproof));
    let _ = Action::Pass;
}

#[test]
fn only_small_creatures_combat_damage_is_prevented() {
    for (pt, prevented) in [((2, 2), true), ((3, 3), false)] {
        let mut t = Table::default();
        let fog = t.card(
            "{G}",
            "Instant",
            None,
            "Prevent all combat damage that would be dealt this turn by creatures with power 2 \
             or less.",
        );
        let beast = t.card("{1}{G}", "Creature — Beast", Some(pt), "");
        let mut g = Game::new(t);
        g.lands(1);
        let fog = g.put(fog, P0, Zone::Hand);
        let beast = g.put(beast, P0, Zone::Battlefield);
        g.main();
        g.cast(fog, &[]);
        let before = g.life(P1);
        g.combat(&[beast], &[], &[], &[]);
        let dealt = before - g.life(P1);
        assert_eq!(dealt == 0, prevented, "{pt:?}");
    }
}
