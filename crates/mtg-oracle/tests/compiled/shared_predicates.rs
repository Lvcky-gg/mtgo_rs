use super::harness::*;
use mtg_core::{Keyword, Step, Target, Zone};

#[test]
fn smallpox_affects_both_players_and_continues_when_a_player_cannot_sacrifice() {
    for opponent_has_permanents in [false, true] {
        let mut t = Table::default();
        let pox = t.card("{0}", "Sorcery", None, "Each player loses 1 life, discards a card, sacrifices a creature of their choice, then sacrifices a land of their choice.");
        let bear = t.bear();
        let land = t.mountain();
        let mut g = Game::new(t);
        for player in [P0, P1] {
            g.put(bear, player, Zone::Hand);
            if player == P0 || opponent_has_permanents {
                g.put(bear, player, Zone::Battlefield);
                g.put(land, player, Zone::Battlefield);
            }
        }
        let spell = g.put(pox, P0, Zone::Hand);
        g.main();
        let hand = [g.count(Zone::Hand, P0), g.count(Zone::Hand, P1)];
        g.cast(spell, &[]);
        assert_eq!(g.life(P0), 19);
        assert_eq!(g.life(P1), 19);
        assert_eq!(g.count(Zone::Hand, P0), hand[0] - 2);
        assert_eq!(g.count(Zone::Hand, P1), hand[1] - 1);
        assert!(g.engine.state.battlefield().is_empty());
    }
}

#[test]
fn wind_shear_keeps_the_pump_on_selected_flyers_after_removing_flying() {
    let mut t = Table::default();
    let shear = t.card(
        "{0}",
        "Instant",
        None,
        "Attacking creatures with flying get -2/-2 and lose flying until end of turn.",
    );
    let flyer = t.card("{0}", "Creature", Some((4, 4)), "Flying");
    let ground = t.card("{0}", "Creature", Some((4, 4)), "");
    let mut g = Game::new(t);
    let attacking_flyer = g.put(flyer, P0, Zone::Battlefield);
    let idle_flyer = g.put(flyer, P0, Zone::Battlefield);
    let ground = g.put(ground, P0, Zone::Battlefield);
    let shear = g.put(shear, P0, Zone::Hand);
    g.main();
    for attacker in [attacking_flyer, ground] {
        g.engine
            .state
            .combat
            .attackers
            .insert(attacker, Target::Player(P1));
    }
    g.cast(shear, &[]);
    assert_eq!(g.pt(attacking_flyer), (2, 2));
    assert!(!g.has(attacking_flyer, Keyword::Flying));
    assert_eq!(g.pt(idle_flyer), (4, 4));
    assert!(g.has(idle_flyer, Keyword::Flying));
    assert_eq!(g.pt(ground), (4, 4));
    g.until(P1, Step::PrecombatMain);
    assert_eq!(g.pt(attacking_flyer), (4, 4));
    assert!(g.has(attacking_flyer, Keyword::Flying));
}

#[test]
fn archon_of_cruelty_keeps_the_opponents_actions_and_controllers_rewards_separate() {
    let mut t = Table::default();
    let archon = t.card("{0}", "Creature", Some((6, 6)), "Flying\nWhenever this creature enters or attacks, target opponent sacrifices a creature or planeswalker of their choice, discards a card, and loses 3 life. You draw a card and gain 3 life.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(bear, P1, Zone::Battlefield);
    g.put(bear, P1, Zone::Hand);
    let spell = g.put(archon, P0, Zone::Hand);
    g.main();
    let our_hand = g.count(Zone::Hand, P0);
    let their_hand = g.count(Zone::Hand, P1);
    let their_graveyard = g.count(Zone::Graveyard, P1);
    g.cast(spell, &[Target::Player(P1)]);
    assert_eq!(g.life(P0), 23);
    assert_eq!(g.life(P1), 17);
    assert_eq!(g.count(Zone::Hand, P0), our_hand);
    assert_eq!(g.count(Zone::Hand, P1), their_hand - 1);
    assert_eq!(g.count(Zone::Graveyard, P1), their_graveyard + 2);
    assert!(g.find(bear).is_none());
}

#[test]
fn mind_drain_reuses_one_target_for_discard_mill_and_life_loss() {
    let mut t = Table::default();
    let drain = t.card(
        "{0}",
        "Sorcery",
        None,
        "Target opponent discards two cards, mills a card, and loses 1 life. You gain 1 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(bear, P1, Zone::Hand);
    g.put(bear, P1, Zone::Hand);
    let spell = g.put(drain, P0, Zone::Hand);
    let hand = g.count(Zone::Hand, P1);
    let library = g.count(Zone::Library, P1);
    let graveyard = g.count(Zone::Graveyard, P1);
    g.main();
    g.cast(spell, &[Target::Player(P1)]);
    assert_eq!(g.count(Zone::Hand, P1), hand - 2);
    assert_eq!(g.count(Zone::Library, P1), library - 1);
    assert_eq!(g.count(Zone::Graveyard, P1), graveyard + 3);
    assert_eq!(g.life(P1), 19);
    assert_eq!(g.life(P0), 21);
}

#[test]
fn canopy_dragon_gains_flying_and_loses_trample_for_the_same_duration() {
    let mut t = Table::default();
    let dragon = t.card(
        "{0}",
        "Creature",
        Some((4, 4)),
        "Trample\n{0}: This creature gains flying and loses trample until end of turn.",
    );
    let mut g = Game::new(t);
    let dragon = g.put(dragon, P0, Zone::Battlefield);
    g.main();
    g.act(activate(dragon, 1), &[], &[]);
    assert!(g.has(dragon, Keyword::Flying));
    assert!(!g.has(dragon, Keyword::Trample));
    g.until(P1, Step::PrecombatMain);
    assert!(!g.has(dragon, Keyword::Flying));
    assert!(g.has(dragon, Keyword::Trample));
}

#[test]
fn gargoyle_sentinel_loses_defender_and_gains_flying_until_end_of_turn() {
    let mut t = Table::default();
    let gargoyle = t.card(
        "{0}",
        "Creature",
        Some((3, 3)),
        "Defender\n{0}: Until end of turn, this creature loses defender and gains flying.",
    );
    let mut g = Game::new(t);
    let gargoyle = g.put(gargoyle, P0, Zone::Battlefield);
    g.main();
    g.act(activate(gargoyle, 1), &[], &[]);
    assert!(!g.has(gargoyle, Keyword::Defender));
    assert!(g.has(gargoyle, Keyword::Flying));
    g.until(P1, Step::PrecombatMain);
    assert!(g.has(gargoyle, Keyword::Defender));
    assert!(!g.has(gargoyle, Keyword::Flying));
}

#[test]
fn sky_tethers_static_gain_and_loss_end_when_the_aura_leaves() {
    let mut t = Table::default();
    let tether = t.card(
        "{0}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature has defender and loses flying.",
    );
    let flyer = t.card("{0}", "Creature", Some((2, 2)), "Flying");
    let destroy = t.card("{0}", "Instant", None, "Destroy target enchantment.");
    let mut g = Game::new(t);
    let flyer = g.put(flyer, P1, Zone::Battlefield);
    let tether_card = tether;
    let tether = g.put(tether_card, P0, Zone::Hand);
    let destroy = g.put(destroy, P0, Zone::Hand);
    g.main();
    g.cast(tether, &[Target::Object(flyer)]);
    assert!(g.has(flyer, Keyword::Defender));
    assert!(!g.has(flyer, Keyword::Flying));
    let tether = g.find(tether_card).unwrap();
    g.cast(destroy, &[Target::Object(tether)]);
    assert!(!g.has(flyer, Keyword::Defender));
    assert!(g.has(flyer, Keyword::Flying));
}
