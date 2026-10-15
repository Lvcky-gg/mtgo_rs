use super::harness::*;
use mtg_core::{CounterKind, Event, Keyword, Step, Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn keyword_counters_follow_layer_timestamps_and_survive_cleanup() {
    let mut t = Table::default();
    let bear = t.bear();
    let add = t.card(
        "{0}",
        "Instant",
        None,
        "Put a flying counter on target creature.",
    );
    let lose = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature loses all abilities until end of turn.",
    );
    let remove = t.card(
        "{0}",
        "Instant",
        None,
        "Remove a flying counter from target creature.",
    );
    let mut g = Game::new(t);
    let creature = g.put(bear, P0, Zone::Battlefield);
    let add1 = g.put(add, P0, Zone::Hand);
    let add2 = g.put(add, P0, Zone::Hand);
    let lose = g.put(lose, P0, Zone::Hand);
    let remove1 = g.put(remove, P0, Zone::Hand);
    let remove2 = g.put(remove, P0, Zone::Hand);
    g.main();
    g.cast(add1, &[Target::Object(creature)]);
    assert!(g.has(creature, Keyword::Flying));
    g.cast(lose, &[Target::Object(creature)]);
    assert!(!g.has(creature, Keyword::Flying));
    g.cast(add2, &[Target::Object(creature)]);
    assert!(g.has(creature, Keyword::Flying));
    assert_eq!(
        g.engine.state.objects[&creature].counters[&CounterKind::Keyword(Keyword::Flying)],
        2
    );
    g.cast(remove1, &[Target::Object(creature)]);
    assert!(g.has(creature, Keyword::Flying));
    g.until(P1, Step::PrecombatMain);
    assert!(g.has(creature, Keyword::Flying));
    g.until(P0, Step::PrecombatMain);
    g.cast(remove2, &[Target::Object(creature)]);
    assert!(!g.has(creature, Keyword::Flying));
    assert!(
        g.engine.state.objects[&creature]
            .keyword_counter_timestamps
            .is_empty()
    );
}

#[test]
fn entering_with_an_indestructible_counter_prevents_destruction() {
    let mut t = Table::default();
    let creature = t.card(
        "{0}",
        "Creature",
        Some((2, 2)),
        "This creature enters with an indestructible counter on it.",
    );
    let destroy = t.card("{0}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    let creature_card = creature;
    let creature = g.put(creature_card, P0, Zone::Hand);
    let destroy = g.put(destroy, P0, Zone::Hand);
    g.main();
    g.cast(creature, &[]);
    let creature = g.find(creature_card).unwrap();
    assert!(g.has(creature, Keyword::Indestructible));
    g.cast(destroy, &[Target::Object(creature)]);
    assert!(g.engine.state.objects.contains_key(&creature));
}

#[test]
fn lifelink_counter_changes_combat_damage_and_bounce_clears_it() {
    let mut t = Table::default();
    let bear = t.bear();
    let add = t.card(
        "{0}",
        "Instant",
        None,
        "Put a lifelink counter on target creature.",
    );
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let creature = g.put(bear, P0, Zone::Battlefield);
    let add = g.put(add, P0, Zone::Hand);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.cast(add, &[Target::Object(creature)]);
    g.combat(&[creature], &[], &[], &[]);
    assert_eq!([g.life(P0), g.life(P1)], [22, 18]);
    g.cast(bounce, &[Target::Object(creature)]);
    let creature = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == bear && o.zone == mtg_core::ZoneRef::of(Zone::Hand, P0))
        .unwrap()
        .id;
    assert!(g.engine.state.objects[&creature].counters.is_empty());
    assert!(
        g.engine.state.objects[&creature]
            .keyword_counter_timestamps
            .is_empty()
    );
}

#[test]
fn gift_of_the_viper_places_all_counter_kinds_together_then_untaps() {
    let mut t = Table::default();
    let bear = t.bear();
    let gift = t.card("{0}", "Instant", None, "Put a +1/+1 counter, a reach counter, and a deathtouch counter on target creature. Untap it.");
    let mut g = Game::new(t);
    let creature = g.put(bear, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&creature).unwrap().tapped = true;
    let gift = g.put(gift, P0, Zone::Hand);
    g.main();
    g.cast(gift, &[Target::Object(creature)]);
    assert_eq!(g.pt(creature), (3, 3));
    assert!(g.has(creature, Keyword::Reach));
    assert!(g.has(creature, Keyword::Deathtouch));
    assert!(!g.engine.state.objects[&creature].tapped);
    let placed: Vec<_> = g.engine.log.iter().filter(|e| matches!(e.event, Event::CountersChanged { object, delta: 1, .. } if object == creature)).collect();
    assert_eq!(placed.len(), 3);
    assert!(placed.iter().all(|e| e.at == placed[0].at));
}

#[test]
fn mixed_counter_list_applies_to_each_matching_creature_simultaneously() {
    let mut t = Table::default();
    let bear = t.bear();
    let counters = t.card(
        "{0}",
        "Instant",
        None,
        "Put two +1/+1 counters and a flying counter on each creature you control.",
    );
    let mut g = Game::new(t);
    let ours = [
        g.put(bear, P0, Zone::Battlefield),
        g.put(bear, P0, Zone::Battlefield),
    ];
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let counters = g.put(counters, P0, Zone::Hand);
    g.main();
    g.cast(counters, &[]);
    for creature in ours {
        assert_eq!(g.pt(creature), (4, 4));
        assert!(g.has(creature, Keyword::Flying));
    }
    assert_eq!(g.pt(theirs), (2, 2));
    assert!(!g.has(theirs, Keyword::Flying));
    let placed: Vec<_> = g
        .engine
        .log
        .iter()
        .filter(
            |e| matches!(e.event, Event::CountersChanged { object, .. } if ours.contains(&object)),
        )
        .collect();
    assert_eq!(placed.len(), 4);
    assert!(placed.iter().all(|e| e.at == placed[0].at));
}

#[test]
fn entering_counter_choice_is_mandatory_and_asks_again_on_reanimation() {
    for picked in [0, 1] {
        let mut t = Table::default();
        let creature = t.card("{0}", "Creature", Some((2, 2)), "This creature enters with your choice of a deathtouch counter or a lifelink counter on it.");
        let kill = t.card("{0}", "Instant", None, "Destroy target creature.");
        let reanimate = t.card(
            "{0}",
            "Sorcery",
            None,
            "Return target creature card from your graveyard to the battlefield.",
        );
        let mut g = Game::new(t);
        let spell = g.put(creature, P0, Zone::Hand);
        let kill = g.put(kill, P0, Zone::Hand);
        let reanimate = g.put(reanimate, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Modes(vec![picked])],
        );
        let object = g.find(creature).unwrap();
        assert_eq!(g.has(object, Keyword::Deathtouch), picked == 0);
        assert_eq!(g.has(object, Keyword::Lifelink), picked == 1);
        g.cast(kill, &[Target::Object(object)]);
        let dead = g
            .engine
            .state
            .objects
            .values()
            .find(|o| o.card == creature && o.zone.zone == Zone::Graveyard)
            .unwrap()
            .id;
        g.act(
            Action::Cast { object: reanimate },
            &[Target::Object(dead)],
            &[Answer::Modes(vec![1 - picked])],
        );
        let returned = g.find(creature).unwrap();
        assert_eq!(g.has(returned, Keyword::Deathtouch), picked == 1);
        assert_eq!(g.has(returned, Keyword::Lifelink), picked == 0);
        assert_eq!(g.engine.state.objects[&returned].counters.len(), 1);
    }
}
