use super::harness::*;
use mtg_core::{Keyword, PlayerId, Step, Target, Zone};
use mtg_engine::state::PlayerState;

#[test]
fn and_binds_more_tightly_than_or_in_static_conditions() {
    for artifact_present in [false, true] {
        for enchantment_present in [false, true] {
            for gained_life in [false, true] {
                let mut t = Table::default();
                let creature = t.card("{0}", "Creature", Some((2, 2)), "This creature gets +2/+2 as long as you control an artifact or you control an enchantment and you gained life this turn.");
                let artifact = t.card("{0}", "Artifact", None, "");
                let enchantment = t.card("{0}", "Enchantment", None, "");
                let gain = t.card("{0}", "Instant", None, "You gain 1 life.");
                let mut g = Game::new(t);
                let creature = g.put(creature, P0, Zone::Battlefield);
                if artifact_present {
                    g.put(artifact, P0, Zone::Battlefield);
                }
                if enchantment_present {
                    g.put(enchantment, P0, Zone::Battlefield);
                }
                let gain = g.put(gain, P0, Zone::Hand);
                g.main();
                if gained_life {
                    g.cast(gain, &[]);
                }
                let expected = if artifact_present || (enchantment_present && gained_life) {
                    (4, 4)
                } else {
                    (2, 2)
                };
                assert_eq!(
                    g.pt(creature),
                    expected,
                    "artifact={artifact_present} enchantment={enchantment_present} life={gained_life}"
                );
            }
        }
    }
}

#[test]
fn deserts_hold_triggers_for_a_desert_in_either_zone() {
    for zone in [None, Some(Zone::Battlefield), Some(Zone::Graveyard)] {
        let mut t = Table::default();
        let bear = t.bear();
        let desert = t.card("", "Land — Desert", None, "");
        let hold = t.card("{0}", "Enchantment — Aura", None, "Enchant creature\nWhen this Aura enters, if you control a Desert or there is a Desert card in your graveyard, you gain 3 life.\nEnchanted creature can't attack or block.");
        let mut g = Game::new(t);
        let bear = g.put(bear, P1, Zone::Battlefield);
        if let Some(zone) = zone {
            g.put(desert, P0, zone);
        }
        g.put(desert, P1, Zone::Graveyard);
        let hold = g.put(hold, P0, Zone::Hand);
        g.main();
        g.cast(hold, &[Target::Object(bear)]);
        assert_eq!(g.life(P0), if zone.is_some() { 23 } else { 20 });
    }
}

#[test]
fn low_life_conditions_consider_every_living_opponent_and_both_baron_requirements() {
    let p2 = PlayerId(2);
    for (ours, first, second, departed, enabled) in [
        (30, 20, 11, false, false),
        (29, 20, 10, false, false),
        (30, 20, 10, false, true),
        (30, 10, 20, false, true),
        (30, 20, 10, true, false),
    ] {
        let mut t = Table::default();
        let baron = t.card("{0}", "Creature", Some((4, 4)), "As long as you have 30 or more life and an opponent has 10 or less life, this creature gets +6/+6 and has flying.");
        let mut g = Game::new(t);
        g.engine.state.players.get_mut(&P0).unwrap().life = ours;
        g.engine.state.players.get_mut(&P1).unwrap().life = first;
        let mut player = PlayerState::new(p2, second);
        player.has_lost = departed;
        g.engine.state.players.insert(p2, player);
        g.engine.state.turn_order.push(p2);
        let baron = g.put(baron, P0, Zone::Battlefield);
        g.main();
        assert_eq!(g.pt(baron), if enabled { (10, 10) } else { (4, 4) });
        assert_eq!(g.has(baron, Keyword::Flying), enabled);
    }
}

#[test]
fn momentum_rumbler_gains_first_strike_then_double_strike_on_later_attacks() {
    let mut t = Table::default();
    let rumbler = t.card("{0}", "Creature", Some((3, 3)), "Whenever this creature attacks, if it doesn't have first strike, put a first strike counter on it.\nWhenever this creature attacks, if it has first strike, it gains double strike until end of turn.");
    let mut g = Game::new(t);
    let rumbler = g.put(rumbler, P0, Zone::Battlefield);
    g.main();
    g.combat(&[rumbler], &[], &[], &[]);
    assert!(g.has(rumbler, Keyword::FirstStrike));
    assert!(!g.has(rumbler, Keyword::DoubleStrike));
    assert_eq!(g.life(P1), 17);
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    g.combat(&[rumbler], &[], &[], &[]);
    assert!(g.has(rumbler, Keyword::DoubleStrike));
    assert_eq!(g.life(P1), 11);
    g.until(P1, Step::PrecombatMain);
    assert!(!g.has(rumbler, Keyword::DoubleStrike));
    assert!(g.has(rumbler, Keyword::FirstStrike));
}

#[test]
fn gained_life_threshold_uses_total_gained_rather_than_net_life_change_and_resets() {
    let mut t = Table::default();
    let creature = t.card(
        "{0}",
        "Creature",
        Some((2, 2)),
        "This creature gets +1/+1 as long as you've gained 3 or more life this turn.",
    );
    let gain_two = t.card("{0}", "Instant", None, "You gain 2 life.");
    let gain_one = t.card("{0}", "Instant", None, "You gain 1 life.");
    let lose = t.card("{0}", "Instant", None, "You lose 5 life.");
    let mut g = Game::new(t);
    let creature = g.put(creature, P0, Zone::Battlefield);
    let gain_two = g.put(gain_two, P0, Zone::Hand);
    let gain_one = g.put(gain_one, P0, Zone::Hand);
    let lose = g.put(lose, P0, Zone::Hand);
    g.main();
    g.cast(gain_two, &[]);
    assert_eq!(g.pt(creature), (2, 2));
    g.cast(lose, &[]);
    g.cast(gain_one, &[]);
    assert_eq!(g.life(P0), 18);
    assert_eq!(g.pt(creature), (3, 3));
    g.until(P1, Step::PrecombatMain);
    assert_eq!(g.pt(creature), (2, 2));
}

#[test]
fn absence_of_a_keyword_counter_is_distinct_from_absence_of_the_keyword() {
    let mut t = Table::default();
    let creature = t.card("{0}", "Creature", Some((2, 2)), "Indestructible\nWhenever this creature attacks, if it doesn't have an indestructible counter on it, put an indestructible counter on it.");
    let mut g = Game::new(t);
    let creature = g.put(creature, P0, Zone::Battlefield);
    g.main();
    g.combat(&[creature], &[], &[], &[]);
    let kind = mtg_core::CounterKind::Keyword(Keyword::Indestructible);
    assert_eq!(g.engine.state.objects[&creature].counters[&kind], 1);
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    g.combat(&[creature], &[], &[], &[]);
    assert_eq!(g.engine.state.objects[&creature].counters[&kind], 1);
}

#[test]
fn keyword_dependent_statics_wait_for_layer_dependency_support() {
    let face = mtg_oracle::compile::FaceText {
        name: "Manor Gargoyle",
        card_types: &[mtg_core::CardType::Creature],
        subtypes: &[],
        oracle_text: Some(
            "Defender\nThis creature has indestructible as long as it has defender.\n{1}: Until end of turn, this creature loses defender and gains flying.",
        ),
        mana_cost: "{5}",
    };
    assert!(
        !mtg_oracle::compile::compile(&face, &mtg_oracle::compile::SubtypeNames(vec![]))
            .understood()
    );
}
