//! Exhaust (activate only once), boast (only after attacking this turn, once a turn), and
//! "Exile this artifact" as a cost.

use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};

#[test]
fn an_exhaust_ability_is_activated_only_once() {
    let mut table = Table::default();
    let engine = table.card(
        "{2}{R}",
        "Creature — Bear",
        Some((2, 2)),
        "Exhaust — {1}: Put a +1/+1 counter on this creature.",
    );
    let mut game = Game::new(table);
    game.lands(3);
    let engine = game.put(engine, P0, Zone::Battlefield);
    let actions = game.main();
    assert!(offers(&actions, engine));
    game.act(activate(engine, 0), &[], &[]);
    assert_eq!(game.pt(engine), (3, 3));
    let actions = game.main();
    assert!(!offers(&actions, engine), "exhausted");
    game.until(P1, mtg_core::Step::PrecombatMain);
    let actions = game.main();
    assert!(!offers(&actions, engine), "still exhausted next turn");
}

#[test]
fn boast_needs_an_attack_this_turn_and_works_once() {
    let mut table = Table::default();
    let boaster = table.card(
        "{1}{R}",
        "Creature — Bear",
        Some((2, 2)),
        "Boast — {1}: This creature deals 1 damage to any target.",
    );
    let mut game = Game::new(table);
    game.lands(2);
    let boaster = game.put(boaster, P0, Zone::Battlefield);
    let actions = game.main();
    assert!(!offers(&actions, boaster), "it hasn't attacked");
    game.combat(&[boaster], &[], &[], &[]);
    // At the postcombat main phase's priority.
    let now = |g: &Game| match &g.pending.as_ref().unwrap().kind {
        mtg_engine::choice::ChoiceKind::Priority { legal } => legal.actions.clone(),
        _ => Vec::new(),
    };
    assert!(offers(&now(&game), boaster), "it attacked this turn");
    game.act(activate(boaster, 0), &[Target::Player(P1)], &[]);
    assert_eq!(game.life(P1), 20 - 2 - 1);
    assert!(!offers(&now(&game), boaster), "once each turn");
}

#[test]
fn exiling_itself_pays_for_its_ability() {
    let mut table = Table::default();
    let relic = table.card(
        "{2}",
        "Artifact",
        None,
        "{1}, Exile this artifact: Draw a card.",
    );
    let mut game = Game::new(table);
    game.lands(1);
    let relic = game.put(relic, P0, Zone::Battlefield);
    game.main();
    let hand = game.count(Zone::Hand, P0);
    game.act(activate(relic, 0), &[], &[]);
    assert_eq!(game.count(Zone::Hand, P0), hand + 1);
    assert!(!game.engine.state.objects.contains_key(&relic));
    assert_eq!(
        game.engine
            .state
            .objects_in(ZoneRef::shared(Zone::Exile))
            .len(),
        1
    );
}

#[test]
fn returning_a_land_or_itself_pays_for_an_ability() {
    let mut table = Table::default();
    let tides = table.card(
        "{1}{U}",
        "Enchantment",
        None,
        "{1}, Return a land you control to its owner's hand: Draw a card.\nReturn this \
         enchantment to its owner's hand: You gain 2 life.",
    );
    let mountain = table.mountain();
    let mut game = Game::new(table);
    game.lands(1);
    let tides = game.put(tides, P0, Zone::Battlefield);
    let land = game.put(mountain, P0, Zone::Battlefield);
    game.main();
    let hand = game.count(Zone::Hand, P0);
    game.act(
        activate(tides, 0),
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![land])],
    );
    assert_eq!(
        game.count(Zone::Hand, P0),
        hand + 2,
        "the land and the card drawn"
    );
    assert!(!game.engine.state.objects.contains_key(&land));
    game.act(activate(tides, 1), &[], &[]);
    assert_eq!(game.life(P0), 22);
    assert_eq!(
        game.count(Zone::Hand, P0),
        hand + 3,
        "and the enchantment itself"
    );
}

#[test]
fn milling_and_putting_counters_on_itself_pay_for_abilities() {
    let mut table = Table::default();
    let tinker = table.card(
        "{1}{B}",
        "Creature — Bear",
        Some((3, 3)),
        "{T}, Mill two cards: You gain 1 life.\nPut a -1/-1 counter on this creature: You gain \
         2 life.",
    );
    let mut game = Game::new(table);
    let tinker = game.put(tinker, P0, Zone::Battlefield);
    game.main();
    let library = game.count(Zone::Library, P0);
    game.act(activate(tinker, 0), &[], &[]);
    assert_eq!(game.count(Zone::Library, P0), library - 2);
    assert_eq!(game.count(Zone::Graveyard, P0), 2);
    game.act(activate(tinker, 1), &[], &[]);
    assert_eq!(game.life(P0), 23);
    assert_eq!(game.pt(tinker), (2, 2));
}

#[test]
fn held_down_for_as_long_as_the_source_remains_tapped() {
    let mut table = Table::default();
    let rack = table.card(
        "{3}",
        "Artifact",
        None,
        "{T}: Tap target creature. It doesn't untap during its controller's untap step for as \
         long as this artifact remains tapped.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    let rack = game.put(rack, P0, Zone::Battlefield);
    let theirs = game.put(bear, P1, Zone::Battlefield);
    game.main();
    game.act(activate(rack, 0), &[Target::Object(theirs)], &[]);
    let tapped = |g: &Game| g.engine.state.objects[&theirs].tapped;
    assert!(tapped(&game));
    game.until(P1, mtg_core::Step::PrecombatMain);
    assert!(tapped(&game), "held down while the rack stays tapped");
    // P0's untap step untaps the rack, which ends the effect.
    game.until(P0, mtg_core::Step::PrecombatMain);
    game.until(P1, mtg_core::Step::PrecombatMain);
    assert!(!tapped(&game), "free once the rack untapped");
}

#[test]
fn the_old_oblivion_ring_returns_what_it_exiled() {
    let mut table = Table::default();
    let ring = table.card(
        "{2}{W}",
        "Enchantment",
        None,
        "When this enchantment enters, exile another target nonland permanent.\nWhen this \
         enchantment leaves the battlefield, return the exiled card to the battlefield under its \
         owner's control.",
    );
    let shatter = table.card("{1}{R}", "Instant", None, "Destroy target enchantment.");
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(5);
    let ring = game.put(ring, P0, Zone::Hand);
    let shatter = game.put(shatter, P0, Zone::Hand);
    let theirs = game.put(bear, P1, Zone::Battlefield);
    game.main();
    game.act(
        mtg_engine::actions::Action::Cast { object: ring },
        &[Target::Object(theirs)],
        &[],
    );
    assert!(!game.engine.state.objects.contains_key(&theirs), "exiled");
    let ring = *game
        .engine
        .state
        .battlefield()
        .iter()
        .find(|id| {
            mtg_engine::layers::compute(&game.engine.state, &game.table, **id)
                .is_some_and(|c| c.has_type(mtg_core::CardType::Enchantment))
        })
        .unwrap();
    game.act(
        mtg_engine::actions::Action::Cast { object: shatter },
        &[Target::Object(ring)],
        &[],
    );
    let back: Vec<_> = game
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| game.engine.state.objects[id].controller == P1)
        .collect();
    assert_eq!(back.len(), 1, "the bear is back, under its owner's control");
}
