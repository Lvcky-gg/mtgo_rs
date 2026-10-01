//! Damage spells with more than one recipient, or a counted amount.
use super::harness::*;
use mtg_core::{Target, Zone};

#[test]
fn damage_to_a_creature_and_to_its_controller() {
    let mut t = Table::default();
    let blast = t.card(
        "{2}{R}",
        "Instant",
        None,
        "~ deals 3 damage to target creature and 2 damage to that creature's controller.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let b = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(blast, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(b)]);
    assert!(g.find(bear).is_none());
    assert_eq!(g.life(P1), 18);
    assert_eq!(g.life(P0), 20);
}

#[test]
fn damage_to_two_targets() {
    let mut t = Table::default();
    let arc = t.card(
        "{1}{R}",
        "Instant",
        None,
        "~ deals 2 damage to target creature and 2 damage to target player or planeswalker.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(arc, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(b), Target::Player(P1)]);
    assert!(g.find(bear).is_none());
    assert_eq!(g.life(P1), 18);
}

#[test]
fn a_pinger_that_hurts_its_controller_too() {
    let mut t = Table::default();
    let pinger = t.card(
        "{1}{R}",
        "Creature — Goblin",
        Some((1, 1)),
        "{T}: This creature deals 1 damage to any target and 1 damage to you.",
    );
    let mut g = Game::new(t);
    let p = g.put(pinger, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&p).unwrap().summoning_sick = false;
    g.main();
    g.act(activate(p, 0), &[Target::Player(P1)], &[]);
    assert_eq!((g.life(P0), g.life(P1)), (19, 19));
}

#[test]
fn any_other_target_must_differ_from_the_first() {
    let mut t = Table::default();
    let forked = t.card(
        "{1}{R}",
        "Sorcery",
        None,
        "~ deals 2 damage to any target and 1 damage to any other target.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let spell = g.put(forked, P0, Zone::Hand);
    g.main();
    let face = mtg_ir::PrintedCards::face(&g.table, forked, 0)
        .unwrap()
        .clone();
    let slots = &face.abilities[0].targets;
    assert_eq!(slots.len(), 2);
    let legal = |already: &[Target]| {
        mtg_engine::targeting::legal_targets(
            &g.engine.state,
            &g.table,
            &slots[1],
            spell,
            P0,
            already,
        )
    };
    assert!(legal(&[]).contains(&Target::Player(P1)));
    assert!(!legal(&[Target::Player(P1)]).contains(&Target::Player(P1)));
    g.cast(spell, &[Target::Player(P1), Target::Player(P0)]);
    assert_eq!((g.life(P0), g.life(P1)), (19, 18));
}

#[test]
fn damage_equal_to_the_lands_you_control() {
    let mut t = Table::default();
    let quake = t.card(
        "{2}{R}",
        "Sorcery",
        None,
        "~ deals damage to target creature equal to the number of lands you control.",
    );
    let ogre = t.card("{3}{R}", "Creature — Goblin", Some((4, 4)), "");
    let mut g = Game::new(t);
    g.lands(4);
    let o = g.put(ogre, P1, Zone::Battlefield);
    let spell = g.put(quake, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(o)]);
    assert!(g.find(ogre).is_none(), "four lands, four damage");
}

#[test]
fn a_card_from_a_graveyard_goes_to_the_bottom_of_its_owners_library() {
    let mut t = Table::default();
    let relic = t.card(
        "{1}",
        "Artifact",
        None,
        "{T}: Put target card from a graveyard on the bottom of its owner's library.",
    );
    let bear = t.bear();
    let mountain = t.mountain();
    let mut g = Game::new(t);
    let r = g.put(relic, P0, Zone::Battlefield);
    let dead = g.put(bear, P1, Zone::Graveyard);
    for _ in 0..3 {
        g.put(mountain, P1, Zone::Library);
    }
    g.main();
    g.act(activate(r, 0), &[Target::Object(dead)], &[]);
    assert_eq!(g.count(Zone::Graveyard, P1), 0);
    let library = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Library, P1));
    // Index 0 is the top.
    let bottom = *library.last().expect("library");
    assert_eq!(g.engine.state.objects[&bottom].card, bear, "at the bottom");
}

#[test]
fn a_creature_card_returns_to_the_top_of_your_library() {
    let mut t = Table::default();
    let recall = t.card(
        "{G}",
        "Sorcery",
        None,
        "Put target creature card from your graveyard on top of your library.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let dead = g.put(bear, P0, Zone::Graveyard);
    let spell = g.put(recall, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(dead)]);
    let library = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Library, P0));
    let top = *library.first().expect("library");
    assert_eq!(g.engine.state.objects[&top].card, bear, "on top");
}

#[test]
fn destroy_only_a_big_enough_creature() {
    let mut t = Table::default();
    let fell = t.card(
        "{1}{W}",
        "Sorcery",
        None,
        "Destroy target creature with toughness 4 or greater.",
    );
    let bear = t.bear();
    let ogre = t.card("{3}{G}", "Creature — Bear", Some((4, 4)), "");
    let mut g = Game::new(t);
    let b = g.put(bear, P1, Zone::Battlefield);
    let o = g.put(ogre, P1, Zone::Battlefield);
    g.main();
    let face = mtg_ir::PrintedCards::face(&g.table, fell, 0)
        .unwrap()
        .clone();
    let spec = &face.abilities[0].targets[0];
    let legal = mtg_engine::targeting::legal_targets(&g.engine.state, &g.table, spec, b, P0, &[]);
    assert!(legal.contains(&Target::Object(o)));
    assert!(!legal.contains(&Target::Object(b)));
}

#[test]
fn two_targets_each_get_a_boost() {
    let mut t = Table::default();
    let rally = t.card(
        "{W}",
        "Instant",
        None,
        "Up to two target creatures each get +2/+0 until end of turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let a = g.put(bear, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(rally, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(a), Target::Object(b)]);
    assert_eq!((g.pt(a), g.pt(b)), ((4, 2), (4, 2)));
}

#[test]
fn a_creature_dealt_damage_this_way_is_exiled_if_it_dies() {
    let mut t = Table::default();
    let scorch = t.card(
        "{1}{R}",
        "Instant",
        None,
        "~ deals 3 damage to any target. If a creature dealt damage this way would die this \
         turn, exile it instead.",
    );
    let bear = t.bear();
    let walker = t.loyalty(
        "{2}{R}",
        "Legendary Planeswalker — Chandra",
        2,
        "+1: You gain 1 life.",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let b = g.put(bear, P1, Zone::Battlefield);
    let w = g.put(walker, P1, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&w)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::Loyalty, 2);
    let s1 = g.put(scorch, P0, Zone::Hand);
    let s2 = g.put(scorch, P0, Zone::Hand);
    g.main();
    g.cast(s1, &[Target::Object(b)]);
    assert!(g.find(bear).is_none());
    assert_eq!(g.count(Zone::Graveyard, P1), 0, "the bear was exiled");
    g.cast(s2, &[Target::Object(w)]);
    assert!(g.find(walker).is_none());
    assert_eq!(
        g.count(Zone::Graveyard, P1),
        1,
        "a planeswalker is not a creature dealt damage this way"
    );
}

#[test]
fn costs_less_for_each_instant_and_sorcery_card() {
    for cards in [1usize, 2] {
        let mut t = Table::default();
        let flare = t.card(
            "{3}{R}",
            "Sorcery",
            None,
            "This spell costs {1} less to cast for each instant and sorcery card in your \
             graveyard.\n~ deals 2 damage to any target.",
        );
        let shock = t.card("{R}", "Instant", None, "~ deals 2 damage to any target.");
        let mut g = Game::new(t);
        g.lands(2);
        for _ in 0..cards {
            g.put(shock, P0, Zone::Graveyard);
        }
        let f = g.put(flare, P0, Zone::Hand);
        let castable = g
            .main()
            .contains(&mtg_engine::actions::Action::Cast { object: f });
        assert_eq!(castable, cards == 2, "{cards} in the graveyard");
    }
}

#[test]
fn kicked_it_deals_more_damage_instead() {
    for kicked in [false, true] {
        let mut t = Table::default();
        let lesson = t.card(
            "{1}{R}",
            "Instant",
            None,
            "Kicker {2}\n~ deals 2 damage to target creature. If this spell was kicked, it \
             deals 5 damage to that creature instead.",
        );
        let ogre = t.card("{3}{R}", "Creature — Goblin", Some((4, 4)), "");
        let mut g = Game::new(t);
        g.lands(4);
        let o = g.put(ogre, P1, Zone::Battlefield);
        let s = g.put(lesson, P0, Zone::Hand);
        g.main();
        g.act(
            mtg_engine::actions::Action::Cast { object: s },
            &[Target::Object(o)],
            &[mtg_engine::choice::Answer::Bool(kicked)],
        );
        assert_eq!(g.find(ogre).is_none(), kicked, "5 kills the 4/4, 2 doesn't");
    }
}

#[test]
fn counters_on_the_equipment_scale_its_bonus_and_instead_names_the_equipped_creature() {
    let mut t = Table::default();
    let blade = t.card(
        "{2}",
        "Artifact — Equipment",
        None,
        "Equipped creature gets +1/+1 for each charge counter on this Equipment.\nWhenever a \
         creature dies, put a +1/+1 counter on equipped creature. If equipped creature is a \
         Zombie, put two +1/+1 counters on it instead.",
    );
    let zombie = t.card("{B}", "Creature — Zombie", Some((1, 1)), "");
    let bear = t.bear();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(1);
    let z = g.put(zombie, P0, Zone::Battlefield);
    let b = g.put(blade, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&b).unwrap().attached_to = Some(z);
    g.engine
        .state
        .objects
        .get_mut(&b)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::Other(0), 2);
    let victim = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(bolt, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(z), (3, 3), "two charge counters");
    g.cast(s, &[Target::Object(victim)]);
    assert_eq!(g.pt(z), (5, 5), "two +1/+1 counters on the Zombie, not one");
}
