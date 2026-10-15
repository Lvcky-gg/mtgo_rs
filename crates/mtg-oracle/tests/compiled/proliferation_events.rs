use super::harness::*;
use mtg_core::{CounterKind, Event, Keyword, Step, Zone};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn proliferating_without_adding_counters_still_triggers_once_per_action() {
    for (text, times) in [
        ("Proliferate.", 1),
        ("Proliferate twice.", 2),
        ("Proliferate, then proliferate again.", 2),
    ] {
        let mut t = Table::default();
        let aspirant = t.card(
            "{0}",
            "Creature",
            Some((1, 3)),
            "Whenever you proliferate, each opponent loses 2 life and you gain 2 life.",
        );
        let spell = t.card("{0}", "Instant", None, text);
        let mut g = Game::new(t);
        g.put(aspirant, P0, Zone::Battlefield);
        g.put(aspirant, P1, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        g.cast(spell, &[]);
        assert_eq!(
            [g.life(P0), g.life(P1)],
            [20 + 2 * times, 20 - 2 * times],
            "{text}"
        );
        assert_eq!(
            g.engine
                .log
                .iter()
                .filter(|e| matches!(e.event, Event::Proliferated { player: P0 }))
                .count(),
            times as usize
        );
    }
}

#[test]
fn successive_proliferations_choose_new_sets_and_each_triggers_a_draw() {
    let mut t = Table::default();
    let bear = t.bear();
    let watcher = t.card(
        "{0}",
        "Creature",
        Some((1, 3)),
        "Whenever you proliferate, draw a card.",
    );
    let twice = t.card("{0}", "Instant", None, "Proliferate twice.");
    let mut g = Game::new(t);
    g.put(watcher, P0, Zone::Battlefield);
    let first = g.put(bear, P0, Zone::Battlefield);
    let second = g.put(bear, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&first)
        .unwrap()
        .counters
        .insert(CounterKind::PlusOnePlusOne, 1);
    g.engine
        .state
        .objects
        .get_mut(&second)
        .unwrap()
        .counters
        .insert(CounterKind::Keyword(Keyword::Flying), 1);
    let twice = g.put(twice, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: twice },
        &[],
        &[Answer::Objects(vec![first]), Answer::Objects(vec![second])],
    );
    assert_eq!(g.pt(first), (4, 4));
    assert_eq!(g.pt(second), (2, 2));
    assert_eq!(
        g.engine.state.objects[&second].counters[&CounterKind::Keyword(Keyword::Flying)],
        2
    );
    assert!(g.has(second, Keyword::Flying));
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
}

#[test]
fn selected_player_gets_both_energy_and_poison_while_declined_player_gets_neither() {
    let mut t = Table::default();
    let spell = t.card("{0}", "Instant", None, "Proliferate.");
    let mut g = Game::new(t);
    for player in [P0, P1] {
        let state = g.engine.state.players.get_mut(&player).unwrap();
        state.poison = 1;
        state.energy = 2;
    }
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Bool(true), Answer::Bool(false)],
    );
    assert_eq!(
        (
            g.engine.state.player(P0).poison,
            g.engine.state.player(P0).energy
        ),
        (2, 3)
    );
    assert_eq!(
        (
            g.engine.state.player(P1).poison,
            g.engine.state.player(P1).energy
        ),
        (1, 2)
    );
}

#[test]
fn agent_frank_horrigan_becomes_indestructible_only_after_it_attacks() {
    let mut t = Table::default();
    let agent = t.card("{0}", "Creature", Some((3, 3)), "This creature has indestructible as long as it attacked this turn.\nWhenever this creature enters or attacks, proliferate twice.");
    let mut g = Game::new(t);
    let agent = g.put(agent, P0, Zone::Battlefield);
    g.main();
    assert!(!g.has(agent, Keyword::Indestructible));
    g.combat(&[agent], &[], &[], &[]);
    assert!(g.has(agent, Keyword::Indestructible));
    assert_eq!(
        g.engine
            .log
            .iter()
            .filter(|e| matches!(e.event, Event::Proliferated { player: P0 }))
            .count(),
        2
    );
    g.until(P1, Step::PrecombatMain);
    assert!(!g.has(agent, Keyword::Indestructible));
}

#[test]
fn proliferating_returns_only_your_voidwing_hybrid_from_the_graveyard() {
    let mut t = Table::default();
    let hybrid = t.card(
        "{0}",
        "Creature",
        Some((2, 1)),
        "Flying\nToxic 1\nWhen you proliferate, return this card from your graveyard to your hand.",
    );
    let proliferate = t.card("{0}", "Instant", None, "Proliferate.");
    let mut g = Game::new(t);
    g.put(hybrid, P0, Zone::Graveyard);
    let theirs = g.put(hybrid, P1, Zone::Graveyard);
    let spell = g.put(proliferate, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    assert_eq!(
        g.engine
            .state
            .objects
            .values()
            .filter(|o| o.card == hybrid && o.zone == mtg_core::ZoneRef::of(Zone::Hand, P0))
            .count(),
        1
    );
    assert_eq!(g.engine.state.objects[&theirs].zone.zone, Zone::Graveyard);
}

#[test]
fn instill_furor_checks_the_enchanted_creatures_attack_history_at_its_controllers_end_step() {
    for attacked in [false, true] {
        let mut t = Table::default();
        let bear = t.bear();
        let aura = t.card("{0}", "Enchantment — Aura", None, "Enchant creature\nEnchanted creature has \"At the beginning of your end step, sacrifice this creature unless it attacked this turn.\"");
        let mut g = Game::new(t);
        let creature = g.put(bear, P0, Zone::Battlefield);
        let aura = g.put(aura, P1, Zone::Battlefield);
        g.engine.state.objects.get_mut(&aura).unwrap().attached_to = Some(creature);
        g.main();
        if attacked {
            g.combat(&[creature], &[], &[], &[]);
        }
        g.until(P1, Step::PrecombatMain);
        assert_eq!(g.engine.state.objects.contains_key(&creature), attacked);
    }
}
