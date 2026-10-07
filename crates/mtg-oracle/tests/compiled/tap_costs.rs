//! "Tap an untapped creature you control" and other costs that tap chosen permanents.
use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::choice::Answer;

fn tapped(g: &Game, id: mtg_core::ObjectId) -> bool {
    g.engine.state.objects[&id].tapped
}

#[test]
fn tapping_a_chosen_creature_pays_the_cost() {
    let mut t = Table::default();
    let rider = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((2, 2)),
        "Tap an untapped creature you control: This creature gets +1/+1 until end of turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let r = g.put(rider, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    // Summoning sickness only stops {T} costs of the creature's own abilities (CR 302.6).
    g.engine.state.objects.get_mut(&b).unwrap().summoning_sick = true;
    let actions = g.main();
    assert!(offers(&actions, r));
    g.act(activate(r, 0), &[], &[Answer::Objects(vec![b])]);
    assert!(tapped(&g, b), "the chosen bear paid");
    assert!(!tapped(&g, r));
    assert_eq!(g.pt(r), (3, 3));
    // The rider itself is still untapped and can pay for its own ability.
    let actions = g.until(P0, Step::PrecombatMain);
    assert!(offers(&actions, r));
    g.act(activate(r, 0), &[], &[Answer::Objects(vec![r])]);
    assert!(tapped(&g, r));
    assert_eq!(g.pt(r), (4, 4));
    let actions = g.until(P0, Step::PrecombatMain);
    assert!(!offers(&actions, r), "nothing untapped is left to tap");
}

#[test]
fn the_cost_counts_only_matching_untapped_permanents() {
    for goblins in [1, 2] {
        let mut t = Table::default();
        let caller = t.card(
            "{1}{G}",
            "Creature — Elf",
            Some((1, 1)),
            "Tap two untapped Goblins you control: Draw a card.",
        );
        let goblin = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
        let mut g = Game::new(t);
        let c = g.put(caller, P0, Zone::Battlefield);
        let gs: Vec<_> = (0..goblins)
            .map(|_| g.put(goblin, P0, Zone::Battlefield))
            .collect();
        let actions = g.main();
        assert_eq!(
            offers(&actions, c),
            goblins == 2,
            "the Elf itself doesn't count"
        );
        if goblins == 2 {
            let hand = g.count(Zone::Hand, P0);
            g.act(activate(c, 0), &[], &[Answer::Objects(gs.clone())]);
            assert!(gs.iter().all(|id| tapped(&g, *id)));
            assert_eq!(g.count(Zone::Hand, P0), hand + 1);
        }
    }
}

#[test]
fn with_its_own_tap_symbol_the_source_cant_be_the_creature_tapped() {
    for helper in [false, true] {
        let mut t = Table::default();
        let elder = t.card(
            "{1}{G}",
            "Creature — Elf",
            Some((1, 1)),
            "{T}, Tap an untapped creature you control: Draw a card.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        let e = g.put(elder, P0, Zone::Battlefield);
        let b = helper.then(|| g.put(bear, P0, Zone::Battlefield));
        let actions = g.main();
        assert_eq!(
            offers(&actions, e),
            helper,
            "the elder alone can't pay twice"
        );
        if let Some(b) = b {
            g.act(activate(e, 0), &[], &[Answer::Objects(vec![b])]);
            assert!(tapped(&g, e) && tapped(&g, b));
        }
    }
}

#[test]
fn a_spell_whose_additional_cost_taps_a_creature() {
    let mut t = Table::default();
    let spell = t.card(
        "{R}",
        "Instant",
        None,
        "As an additional cost to cast this spell, tap an untapped creature you control.\n\
         This spell deals 3 damage to any target.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(spell, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.act(
        mtg_engine::actions::Action::Cast { object: s },
        &[mtg_core::Target::Player(P1)],
        &[Answer::Objects(vec![b])],
    );
    assert!(tapped(&g, b));
    assert_eq!(g.life(P1), 17);
}

// ---- kickers paid by choosing ---------------------------------------------------------------

#[test]
fn casualty_sacrifices_a_creature_to_copy_the_spell() {
    for pay in [false, true] {
        let mut t = Table::default();
        let bolt = t.card(
            "{1}{R}",
            "Instant",
            None,
            "Casualty 2\n~ deals 2 damage to target player.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(2);
        let s = g.put(bolt, P0, Zone::Hand);
        let b = g.put(bear, P0, Zone::Battlefield);
        g.main();
        let answers = if pay {
            vec![Answer::Bool(true), Answer::Objects(vec![b])]
        } else {
            vec![Answer::Bool(false)]
        };
        g.act(
            mtg_engine::actions::Action::Cast { object: s },
            &[mtg_core::Target::Player(P1), mtg_core::Target::Player(P1)],
            &answers,
        );
        assert_eq!(g.life(P1), if pay { 16 } else { 18 });
        assert_eq!(g.engine.state.objects.contains_key(&b), !pay);
    }
}

#[test]
fn casualty_on_a_creature_copies_it_as_a_token() {
    let mut t = Table::default();
    let knight = t.card("{2}{B}", "Creature — Soldier", Some((3, 3)), "Casualty 1");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let k = g.put(knight, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.act(
        mtg_engine::actions::Action::Cast { object: k },
        &[],
        &[Answer::Bool(true), Answer::Objects(vec![b])],
    );
    let knights = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            mtg_engine::layers::compute(&g.engine.state, &g.table, *id)
                .is_some_and(|c| c.power == Some(3))
        })
        .count();
    assert_eq!(knights, 2, "the card and a token copy");
}

#[test]
fn a_kicker_paid_by_sacrificing_a_creature() {
    let mut t = Table::default();
    let spell = t.card(
        "{1}{W}",
        "Sorcery",
        None,
        "Kicker—Sacrifice a creature.\nYou gain 1 life. If this spell was kicked, you gain 3 \
         life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(spell, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.act(
        mtg_engine::actions::Action::Cast { object: s },
        &[],
        &[Answer::Bool(true), Answer::Objects(vec![b])],
    );
    assert!(!g.engine.state.objects.contains_key(&b), "sacrificed");
    assert_eq!(g.life(P0), 24);
}

#[test]
fn conspire_taps_two_creatures_that_share_a_color_to_copy() {
    let mut t = Table::default();
    let bolt = t.card(
        "{1}{R}",
        "Instant",
        None,
        "Conspire\n~ deals 2 damage to target player.",
    );
    let goblin = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(bolt, P0, Zone::Hand);
    let g1 = g.put(goblin, P0, Zone::Battlefield);
    let g2 = g.put(goblin, P0, Zone::Battlefield);
    let e = g.put(elf, P0, Zone::Battlefield);
    g.main();
    g.act(
        mtg_engine::actions::Action::Cast { object: s },
        &[mtg_core::Target::Player(P1), mtg_core::Target::Player(P1)],
        &[Answer::Bool(true), Answer::Objects(vec![g1, g2])],
    );
    assert_eq!(g.life(P1), 16, "copied");
    assert!(tapped(&g, g1) && tapped(&g, g2) && !tapped(&g, e));
}

#[test]
fn conspire_needs_two_creatures_of_its_color() {
    let mut t = Table::default();
    let bolt = t.card(
        "{1}{R}",
        "Instant",
        None,
        "Conspire\n~ deals 2 damage to target player.",
    );
    let goblin = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(bolt, P0, Zone::Hand);
    let gob = g.put(goblin, P0, Zone::Battlefield);
    let e = g.put(elf, P0, Zone::Battlefield);
    g.main();
    // Were conspire offered, these answers would pay it with the green Elf.
    g.act(
        mtg_engine::actions::Action::Cast { object: s },
        &[mtg_core::Target::Player(P1), mtg_core::Target::Player(P1)],
        &[Answer::Bool(true), Answer::Objects(vec![gob, e])],
    );
    assert_eq!(g.life(P1), 18);
}

#[test]
fn devour_sacrifices_creatures_as_it_enters_for_counters() {
    for eat in [false, true] {
        let mut t = Table::default();
        let maw = t.card("{2}{G}", "Creature — Bear", Some((2, 2)), "Devour 2");
        let goblin = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
        let mut g = Game::new(t);
        g.lands(3);
        let m = g.put(maw, P0, Zone::Hand);
        let gs: Vec<_> = (0..2)
            .map(|_| g.put(goblin, P0, Zone::Battlefield))
            .collect();
        g.main();
        let answer = Answer::Objects(if eat { gs.clone() } else { Vec::new() });
        g.act(
            mtg_engine::actions::Action::Cast { object: m },
            &[],
            &[answer],
        );
        let on = g.find(maw).expect("entered");
        assert_eq!(g.pt(on), if eat { (6, 6) } else { (2, 2) });
        assert_eq!(g.count(Zone::Graveyard, P0), if eat { 2 } else { 0 });
    }
}

#[test]
fn the_sacrificed_creature_is_read_as_it_last_was() {
    let mut t = Table::default();
    let fling = t.card(
        "{1}{R}",
        "Instant",
        None,
        "As an additional cost to cast this spell, sacrifice a creature.\n~ deals damage equal \
         to the sacrificed creature's power to any target.",
    );
    let altar = t.card(
        "{2}",
        "Artifact",
        None,
        "{1}, Sacrifice a creature: You gain life equal to the sacrificed creature's toughness.",
    );
    let big = t.card("{4}", "Creature — Bear", Some((4, 5)), "");
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(fling, P0, Zone::Hand);
    let a = g.put(altar, P0, Zone::Battlefield);
    let b1 = g.put(big, P0, Zone::Battlefield);
    let b2 = g.put(big, P0, Zone::Battlefield);
    g.main();
    g.act(
        mtg_engine::actions::Action::Cast { object: s },
        &[mtg_core::Target::Player(P1)],
        &[Answer::Objects(vec![b1])],
    );
    assert_eq!(g.life(P1), 16);
    g.act(activate(a, 0), &[], &[Answer::Objects(vec![b2])]);
    assert_eq!(g.life(P0), 25);
}

#[test]
fn draw_cards_equal_to_the_sacrificed_creatures_power() {
    let mut t = Table::default();
    let spell = t.card(
        "{1}{G}",
        "Sorcery",
        None,
        "As an additional cost to cast this spell, sacrifice a creature.\nDraw cards equal to \
         the sacrificed creature's power.",
    );
    let big = t.card("{4}", "Creature — Bear", Some((3, 5)), "");
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(spell, P0, Zone::Hand);
    let b = g.put(big, P0, Zone::Battlefield);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(
        mtg_engine::actions::Action::Cast { object: s },
        &[],
        &[Answer::Objects(vec![b])],
    );
    assert_eq!(g.count(Zone::Hand, P0), hand - 1 + 3);
}

#[test]
fn station_charges_a_spacecraft_until_it_is_a_creature() {
    let mut t = Table::default();
    let ship = t.card(
        "{2}{R}",
        "Artifact — Spacecraft",
        Some((3, 3)),
        "Station (Tap another creature you control: Put charge counters equal to its power on \
         this Spacecraft. Station only as a sorcery. It's an artifact creature at 3+.)\n3+ | \
         Flying, haste",
    );
    let small = t.card("{1}", "Creature — Bear", Some((2, 2)), "");
    let big = t.card("{4}", "Creature — Bear", Some((4, 4)), "");
    let mut g = Game::new(t);
    let s = g.put(ship, P0, Zone::Battlefield);
    let a = g.put(small, P0, Zone::Battlefield);
    let b = g.put(big, P0, Zone::Battlefield);
    g.main();
    let is_creature = |g: &Game| {
        mtg_engine::layers::compute(&g.engine.state, &g.table, s)
            .unwrap()
            .has_type(mtg_core::CardType::Creature)
    };
    assert!(!is_creature(&g));
    // Two counters from the 2/2: not yet.
    g.act(activate(s, 0), &[], &[Answer::Objects(vec![a])]);
    assert!(!is_creature(&g));
    assert!(!g.has(s, mtg_core::Keyword::Flying));
    // Four more from the 4/4: six, past 3+.
    g.act(activate(s, 0), &[], &[Answer::Objects(vec![b])]);
    assert!(is_creature(&g));
    assert_eq!(g.pt(s), (3, 3));
    assert!(g.has(s, mtg_core::Keyword::Flying));
    assert!(g.has(s, mtg_core::Keyword::Haste));
    let charge: i32 = g.engine.state.objects[&s].counters.values().sum();
    assert_eq!(charge, 6);
}
