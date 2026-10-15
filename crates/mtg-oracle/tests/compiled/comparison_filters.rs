use super::harness::*;
use mtg_core::{CounterKind, Target, Zone, ZoneRef};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

#[test]
fn comparison_boundaries_distinguish_strict_and_inclusive_power_and_toughness() {
    for metric in ["power", "toughness"] {
        for (phrase, removed) in [
            ("less than", [true, false, false]),
            ("less than or equal to", [true, true, false]),
            ("greater than", [false, false, true]),
            ("greater than or equal to", [false, true, true]),
        ] {
            let mut t = Table::default();
            let removal = t.card(
                "{0}",
                "Sorcery",
                None,
                &format!("Exile each creature with {metric} {phrase} 2."),
            );
            let creatures: Vec<_> = (1..=3)
                .map(|size| t.card("{0}", "Creature", Some((size, size)), ""))
                .collect();
            let mut g = Game::new(t);
            let objects: Vec<_> = creatures
                .into_iter()
                .map(|card| g.put(card, P1, Zone::Battlefield))
                .collect();
            let spell = g.put(removal, P0, Zone::Hand);
            g.main();
            g.cast(spell, &[]);
            for (index, object) in objects.into_iter().enumerate() {
                assert_eq!(
                    g.engine.state.objects.contains_key(&object),
                    !removed[index],
                    "{metric} {phrase} {index}"
                );
            }
        }
    }
}

#[test]
fn ensnaring_bridge_uses_its_controllers_live_hand_count_and_allows_equal_power() {
    let mut t = Table::default();
    let bridge = t.card(
        "{0}",
        "Artifact",
        None,
        "Creatures with power greater than the number of cards in your hand can't attack.",
    );
    let draw = t.card("{0}", "Instant", None, "Draw two cards.");
    let discard = t.card("{0}", "Instant", None, "Discard two cards.");
    let creatures: Vec<_> = (2..=4)
        .map(|power| t.card("{0}", "Creature", Some((power, power)), ""))
        .collect();
    let mut g = Game::new(t);
    g.put(bridge, P0, Zone::Battlefield);
    let creatures: Vec<_> = creatures
        .into_iter()
        .map(|card| g.put(card, P0, Zone::Battlefield))
        .collect();
    let draw = g.put(draw, P0, Zone::Hand);
    let discard = g.put(discard, P0, Zone::Hand);
    g.main();
    assert_eq!(g.count(Zone::Hand, P0), 3);
    let allowed = mtg_engine::combat::eligible_attackers(&g.engine.state, &g.table, P0);
    assert!(allowed.contains(&creatures[0]));
    assert!(allowed.contains(&creatures[1]));
    assert!(!allowed.contains(&creatures[2]));
    g.cast(draw, &[]);
    assert_eq!(g.count(Zone::Hand, P0), 4);
    assert!(
        mtg_engine::combat::eligible_attackers(&g.engine.state, &g.table, P0)
            .contains(&creatures[2])
    );
    g.cast(discard, &[]);
    assert_eq!(g.count(Zone::Hand, P0), 1);
    assert!(mtg_engine::combat::eligible_attackers(&g.engine.state, &g.table, P0).is_empty());
}

#[test]
fn champion_of_lambholt_uses_its_current_power_for_blocking_restrictions() {
    let mut t = Table::default();
    let champion = t.card(
        "{0}",
        "Creature",
        Some((1, 1)),
        "Creatures with power less than this creature's power can't block creatures you control.",
    );
    let bear = t.bear();
    let blocker = t.card("{0}", "Creature", Some((1, 1)), "");
    let pump = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature gets +2/+2 until end of turn.",
    );
    let mut g = Game::new(t);
    let champion = g.put(champion, P0, Zone::Battlefield);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    let blocker = g.put(blocker, P1, Zone::Battlefield);
    let pump = g.put(pump, P0, Zone::Hand);
    g.main();
    g.engine
        .state
        .combat
        .attackers
        .insert(attacker, Target::Player(P1));
    assert!(mtg_engine::combat::can_block(
        &g.engine.state,
        &g.table,
        blocker,
        attacker
    ));
    g.cast(pump, &[Target::Object(champion)]);
    assert_eq!(g.pt(champion), (3, 3));
    assert!(!mtg_engine::combat::can_block(
        &g.engine.state,
        &g.table,
        blocker,
        attacker
    ));
}

#[test]
fn beseech_the_queen_filters_search_candidates_by_the_current_land_count() {
    let mut t = Table::default();
    let beseech = t.card("{0}", "Sorcery", None, "Search your library for a card with mana value less than or equal to the number of lands you control, reveal it, put it into your hand, then shuffle.");
    let cheap = t.card("{2}", "Artifact", None, "");
    let expensive = t.card("{3}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let spell = g.put(beseech, P0, Zone::Hand);
    g.main();
    let cheap_object = g.put(cheap, P0, Zone::Library);
    let expensive = g.put(expensive, P0, Zone::Library);
    g.act_holding(Action::Cast { object: spell }, &[]);
    let chosen = std::cell::Cell::new(false);
    g.drive(
        |_, choice| {
            if let ChoiceKind::ChooseObjects { from, .. } = &choice.kind {
                assert!(from.contains(&cheap_object));
                assert!(!from.contains(&expensive));
                chosen.set(true);
                return Some(Answer::Objects(vec![cheap_object]));
            }
            None
        },
        |g| chosen.get() && g.stack().is_empty(),
    );
    assert!(
        g.engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == cheap)
    );
}

#[test]
fn earthshaker_khenras_dynamic_target_bound_is_rechecked_on_resolution() {
    for increase_target in [false, true] {
        let mut t = Table::default();
        let khenra = t.card("{0}", "Creature", Some((2, 1)), "When this creature enters, target creature with power less than or equal to this creature's power can't block this turn.");
        let bear = t.bear();
        let pump = t.card(
            "{0}",
            "Instant",
            None,
            "Target creature gets +2/+2 until end of turn.",
        );
        let mut g = Game::new(t);
        let attacker = g.put(bear, P0, Zone::Battlefield);
        let target = g.put(bear, P1, Zone::Battlefield);
        let spell = g.put(khenra, P0, Zone::Hand);
        let pump = g.put(pump, P0, Zone::Hand);
        g.main();
        g.act_holding(Action::Cast { object: spell }, &[]);
        let question = g.pending.take().unwrap();
        g.engine
            .answer(&g.table, question.id, Answer::Pass)
            .unwrap();
        g.drive(
            |_, question| {
                if matches!(question.kind, ChoiceKind::ChooseTargets { .. }) {
                    Some(Answer::Targets(vec![vec![Target::Object(target)]]))
                } else {
                    None
                }
            },
            |g| g.find(khenra).is_some() && !g.stack().is_empty(),
        );
        if increase_target {
            g.cast(pump, &[Target::Object(target)]);
        } else {
            let question = g.pending.take().unwrap();
            g.engine
                .answer(&g.table, question.id, Answer::Pass)
                .unwrap();
            g.drive(|_, _| None, |g| g.stack().is_empty());
        }
        g.engine
            .state
            .combat
            .attackers
            .insert(attacker, Target::Player(P1));
        assert_eq!(
            mtg_engine::combat::can_block(&g.engine.state, &g.table, target, attacker),
            increase_target
        );
    }
}

#[test]
fn death_trigger_target_bounds_use_the_sources_last_known_power() {
    let mut t = Table::default();
    let panda = t.card("{0}", "Creature — Bear", Some((3, 2)), "When this creature dies, return another target non-Bear creature card with mana value less than or equal to this creature's power from your graveyard to the battlefield.");
    let elf = t.card("{5}", "Creature — Elf", Some((5, 5)), "");
    let pump = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature gets +3/+3 until end of turn.",
    );
    let destroy = t.card("{0}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    let source = g.put(panda, P0, Zone::Battlefield);
    let dead = g.put(elf, P0, Zone::Graveyard);
    let pump = g.put(pump, P0, Zone::Hand);
    let destroy = g.put(destroy, P0, Zone::Hand);
    g.main();
    g.cast(pump, &[Target::Object(source)]);
    assert_eq!(g.pt(source), (6, 5));
    g.act(
        Action::Cast { object: destroy },
        &[Target::Object(source), Target::Object(dead)],
        &[],
    );
    use mtg_engine::eval::CharacteristicsSource;
    assert_eq!(
        mtg_engine::eval::ComputedChars(&g.table)
            .characteristics(&g.engine.state, source)
            .unwrap()
            .power,
        Some(6)
    );
    assert!(g.find(elf).is_some());
}

#[test]
fn pollywog_prodigys_cast_filter_evaluates_source_power_including_counters() {
    for (cost, triggers) in [("{0}", true), ("{2}", true), ("{3}", false)] {
        let mut t = Table::default();
        let prodigy = t.card("{0}", "Creature", Some((2, 3)), "Whenever an opponent casts a noncreature spell with mana value less than this creature's power, draw a card.");
        let spell = t.card(cost, "Instant", None, "You gain 1 life.");
        let rock = t.card("{0}", "Artifact", None, "{T}: Add {C}{C}{C}.");
        let mut g = Game::new(t);
        let source = g.put(prodigy, P0, Zone::Battlefield);
        g.engine
            .state
            .objects
            .get_mut(&source)
            .unwrap()
            .counters
            .insert(CounterKind::PlusOnePlusOne, 1);
        g.put(rock, P1, Zone::Battlefield);
        let spell = g.put(spell, P1, Zone::Hand);
        g.until(P1, mtg_core::Step::PrecombatMain);
        let hand = g.count(Zone::Hand, P0);
        g.act(Action::Cast { object: spell }, &[], &[]);
        assert_eq!(g.count(Zone::Hand, P0), hand + usize::from(triggers));
    }
}
