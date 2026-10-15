use super::harness::*;
use mtg_core::{CounterKind, Event, Keyword, Step, Zone, ZoneRef};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};
use mtg_ir::PrintedCards;

#[test]
fn separate_landcycling_abilities_search_their_own_types_and_cycle_only_once() {
    for ability in [0, 1] {
        let mut t = Table::default();
        let cycler = t.card(
            "{0}",
            "Creature",
            Some((3, 6)),
            "Plainscycling {2}, islandcycling {2}",
        );
        let plains = t.card("", "Basic Land — Plains", None, "");
        let island = t.card("", "Basic Land — Island", None, "");
        let dual = t.card("", "Land — Island Swamp", None, "");
        let forest = t.card("", "Basic Land — Forest", None, "");
        let mut g = Game::new(t);
        g.lands(2);
        let source = g.put(cycler, P0, Zone::Hand);
        g.main();
        let p = g.put(plains, P0, Zone::Library);
        let i = g.put(island, P0, Zone::Library);
        let d = g.put(dual, P0, Zone::Library);
        g.put(forest, P0, Zone::Library);
        let wanted = if ability == 0 { p } else { d };
        let wanted_card = if ability == 0 { plains } else { dual };
        let start = g.engine.log.len();
        g.act_holding(activate(source, ability), &[]);
        let chosen = std::cell::Cell::new(false);
        g.drive(
            |_, question| {
                if let ChoiceKind::ChooseObjects { from, min, max } = &question.kind {
                    let expected = if ability == 0 { vec![p] } else { vec![i, d] };
                    assert_eq!(from, &expected);
                    assert_eq!((*min, *max), (0, 1));
                    chosen.set(true);
                    return Some(Answer::Objects(vec![wanted]));
                }
                None
            },
            |g| chosen.get() && g.stack().is_empty(),
        );
        let events = &g.engine.log[start..];
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e.event, Event::Cycled { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e.event, Event::Shuffled { .. }))
                .count(),
            1
        );
        assert!(!events.iter().any(|e| matches!(e.event, Event::Drew { .. })));
        assert!(
            g.engine
                .state
                .objects_in(ZoneRef::of(Zone::Hand, P0))
                .iter()
                .any(|id| g.engine.state.objects[id].card == wanted_card)
        );
        assert_eq!(
            g.engine
                .state
                .objects_in(ZoneRef::of(Zone::Graveyard, P0))
                .iter()
                .filter(|id| g.engine.state.objects[id].card == cycler)
                .count(),
            1
        );
    }
}

#[test]
fn declining_a_landcycling_search_still_discards_and_shuffles() {
    let mut t = Table::default();
    let cycler = t.card(
        "{0}",
        "Creature",
        Some((3, 6)),
        "Plainscycling {0}, islandcycling {0}",
    );
    let plains = t.card("", "Basic Land — Plains", None, "");
    let mut g = Game::new(t);
    let source = g.put(cycler, P0, Zone::Hand);
    g.main();
    g.put(plains, P0, Zone::Library);
    let hand = g.count(Zone::Hand, P0);
    let start = g.engine.log.len();
    g.act(activate(source, 0), &[], &[Answer::Objects(vec![])]);
    assert_eq!(g.count(Zone::Hand, P0), hand - 1);
    assert!(
        g.engine.log[start..]
            .iter()
            .any(|e| matches!(e.event, Event::Shuffled { player } if player == P0))
    );
    assert!(!g.engine.state.objects.contains_key(&source));
}

#[test]
fn neither_landcycling_ability_functions_from_the_battlefield_or_graveyard() {
    for zone in [Zone::Battlefield, Zone::Graveyard] {
        let mut t = Table::default();
        let cycler = t.card(
            "{0}",
            "Creature",
            Some((3, 6)),
            "Plainscycling {0}, islandcycling {0}",
        );
        let mut g = Game::new(t);
        let source = g.put(cycler, P0, zone);
        let actions = g.main();
        assert!(!actions.contains(&activate(source, 0)));
        assert!(!actions.contains(&activate(source, 1)));
    }
}

#[test]
fn kindred_discovery_draws_on_matching_entries_and_attacks_only_for_its_controller() {
    let mut t = Table::default();
    let discovery = t.card("{0}", "Enchantment", None, "As this enchantment enters, choose a creature type.\nWhenever a creature you control of the chosen type enters or attacks, draw a card.");
    let elf = t.card("{0}", "Creature — Elf", Some((1, 1)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let discovery = g.put(discovery, P0, Zone::Hand);
    let ours = g.put(elf, P0, Zone::Hand);
    let other_type = g.put(bear, P0, Zone::Hand);
    let theirs = g.put(elf, P1, Zone::Hand);
    g.main();
    let mut names: Vec<_> = (0..1024)
        .filter_map(|i| g.table.subtype_name(mtg_core::Subtype(i)))
        .filter(|name| mtg_core::is_creature_type(name))
        .collect();
    names.sort_unstable();
    names.dedup();
    let elf_choice = names.iter().position(|name| *name == "Elf").unwrap() as u8;
    g.act(
        Action::Cast { object: discovery },
        &[],
        &[Answer::Modes(vec![elf_choice])],
    );
    let hand = g.count(Zone::Hand, P0);
    g.cast(ours, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand);
    g.cast(other_type, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand - 1);
    g.until(P1, Step::PrecombatMain);
    let hand = g.count(Zone::Hand, P0);
    g.act(Action::Cast { object: theirs }, &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand);
    g.until(P0, Step::PrecombatMain);
    let ours = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].card == elf && g.engine.state.objects[id].owner == P0)
        .unwrap();
    let bear = g.find(bear).unwrap();
    let hand = g.count(Zone::Hand, P0);
    g.combat(&[ours, bear], &[], &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
}

#[test]
fn commander_anthem_filters_and_turn_conditions_apply_only_to_the_controllers_commander() {
    let mut t = Table::default();
    let falthis = t.card(
        "{0}",
        "Creature",
        Some((2, 2)),
        "Commanders you control have menace and deathtouch.",
    );
    let anara = t.card(
        "{0}",
        "Creature",
        Some((2, 2)),
        "During your turn, commanders you control have indestructible.",
    );
    let ours = t.card("{0}", "Legendary Creature — Elf", Some((2, 2)), "");
    let theirs = t.card("{0}", "Legendary Creature — Bear", Some((2, 2)), "");
    let ordinary = t.bear();
    let mut g = Game::new(t);
    g.engine.state.commander.commanders.insert(P0, ours);
    g.engine.state.commander.commanders.insert(P1, theirs);
    g.put(falthis, P0, Zone::Battlefield);
    g.put(anara, P0, Zone::Battlefield);
    let ours = g.put(ours, P0, Zone::Battlefield);
    let theirs = g.put(theirs, P1, Zone::Battlefield);
    let ordinary = g.put(ordinary, P0, Zone::Battlefield);
    g.main();
    for keyword in [
        Keyword::Menace,
        Keyword::Deathtouch,
        Keyword::Indestructible,
    ] {
        assert!(g.has(ours, keyword));
        assert!(!g.has(theirs, keyword));
        assert!(!g.has(ordinary, keyword));
    }
    g.until(P1, Step::PrecombatMain);
    assert!(!g.has(ours, Keyword::Indestructible));
    assert!(g.has(ours, Keyword::Menace));
    assert!(g.has(ours, Keyword::Deathtouch));
}

#[test]
fn norns_choirmaster_proliferates_on_a_commanders_entry_and_attack() {
    let mut t = Table::default();
    let choirmaster = t.card(
        "{0}",
        "Creature",
        Some((3, 3)),
        "Flying, first strike\nWhenever a commander you control enters or attacks, proliferate.",
    );
    let commander = t.card("{0}", "Legendary Creature — Elf", Some((1, 1)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    g.engine.state.commander.commanders.insert(P0, commander);
    g.put(choirmaster, P0, Zone::Battlefield);
    let counter_holder = g.put(bear, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&counter_holder)
        .unwrap()
        .counters
        .insert(CounterKind::PlusOnePlusOne, 1);
    let commander_spell = g.put(commander, P0, Zone::Hand);
    let ordinary = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(ordinary, &[]);
    assert_eq!(g.pt(counter_holder), (3, 3));
    g.act(
        Action::Cast {
            object: commander_spell,
        },
        &[],
        &[Answer::Objects(vec![counter_holder])],
    );
    assert_eq!(g.pt(counter_holder), (4, 4));
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    let commander = g.find(commander).unwrap();
    g.combat(
        &[commander],
        &[],
        &[],
        &[Answer::Objects(vec![counter_holder])],
    );
    assert_eq!(g.pt(counter_holder), (5, 5));
}
