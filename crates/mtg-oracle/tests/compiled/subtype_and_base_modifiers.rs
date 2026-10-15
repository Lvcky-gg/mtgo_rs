use super::harness::*;
use mtg_core::{Event, Keyword, Step, Target, Zone, ZoneRef};
use mtg_engine::choice::{Answer, ChoiceKind};
use mtg_ir::PrintedCards;

#[test]
fn senator_peacock_adds_clue_type_and_a_working_sacrifice_ability() {
    let mut t = Table::default();
    let peacock = t.card("{0}", "Creature", Some((3, 3)), "Artifacts you control are Clues in addition to their other types and have \"{2}, Sacrifice this artifact: Draw a card.\"");
    let equipment = t.card("{0}", "Artifact — Equipment", None, "Equip {0}");
    let mut g = Game::new(t);
    g.lands(2);
    g.put(peacock, P0, Zone::Battlefield);
    let ours = g.put(equipment, P0, Zone::Battlefield);
    let theirs = g.put(equipment, P1, Zone::Battlefield);
    let actions = g.main();
    let clue = g.table.subtype_named("Clue").unwrap();
    let equipment = g.table.subtype_named("Equipment").unwrap();
    let ch = g.engine.characteristics(&g.table, ours).unwrap();
    assert!(ch.subtypes.contains(&clue));
    assert!(ch.subtypes.contains(&equipment));
    assert!(
        !g.engine
            .characteristics(&g.table, theirs)
            .unwrap()
            .subtypes
            .contains(&clue)
    );
    let action = actions
        .into_iter()
        .find(|action| {
            matches!(action,
        mtg_engine::actions::Action::ActivateAbility { source, ability }
        if *source == ours && ability.0 >= mtg_core::AbilityId::GRANTED_BASE)
        })
        .unwrap();
    let hand = g.count(Zone::Hand, P0);
    g.act(action, &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
    assert!(!g.engine.state.objects.contains_key(&ours));
    assert!(g.engine.state.objects.contains_key(&theirs));
}

#[test]
fn identical_toxic_grants_stack_with_printed_toxic() {
    let mut t = Table::default();
    let creature = t.card("{0}", "Creature", Some((2, 2)), "Toxic 1");
    let aura = t.card(
        "{0}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature has toxic 2.",
    );
    let mut g = Game::new(t);
    let creature = g.put(creature, P0, Zone::Battlefield);
    for _ in 0..2 {
        let aura = g.put(aura, P0, Zone::Battlefield);
        g.engine.state.objects.get_mut(&aura).unwrap().attached_to = Some(creature);
    }
    g.main();
    g.combat(&[creature], &[], &[], &[]);
    assert_eq!(g.life(P1), 18);
    assert_eq!(g.engine.state.player(P1).poison, 5);
}

#[test]
fn granted_toxic_expires_with_the_rest_of_aspirants_ascent() {
    let mut t = Table::default();
    let creature = t.bear();
    let ascent = t.card(
        "{0}",
        "Instant",
        None,
        "Until end of turn, target creature gets +1/+3 and gains flying and toxic 1.",
    );
    let mut g = Game::new(t);
    let creature = g.put(creature, P0, Zone::Battlefield);
    let ascent = g.put(ascent, P0, Zone::Hand);
    g.main();
    g.cast(ascent, &[Target::Object(creature)]);
    assert_eq!(g.pt(creature), (3, 5));
    g.combat(&[creature], &[], &[], &[]);
    assert_eq!(g.life(P1), 17);
    assert_eq!(g.engine.state.player(P1).poison, 1);
    g.until(P1, Step::PrecombatMain);
    assert_eq!(g.pt(creature), (2, 2));
    assert!(!g.has(creature, Keyword::Flying));
    assert!(!g.has(creature, Keyword::Toxic(1)));
    g.until(P0, Step::PrecombatMain);
    g.combat(&[creature], &[], &[], &[]);
    assert_eq!(g.life(P1), 15);
    assert_eq!(g.engine.state.player(P1).poison, 1);
}

#[test]
fn created_mite_has_toxic_and_its_printed_blocking_restriction() {
    let mut t = Table::default();
    let create = t.card("{0}", "Sorcery", None, "Create a 1/1 colorless Phyrexian Mite artifact creature token with toxic 1 and \"This token can't block.\"");
    let attacker = t.bear();
    let mut g = Game::new(t);
    let create = g.put(create, P0, Zone::Hand);
    let attacker = g.put(attacker, P1, Zone::Battlefield);
    g.main();
    g.cast(create, &[]);
    let mites: Vec<_> = g
        .engine
        .state
        .objects
        .values()
        .filter(|object| {
            object.is_token && object.zone.zone == Zone::Battlefield && object.owner == P0
        })
        .map(|object| object.id)
        .collect();
    assert_eq!(mites.len(), 1);
    let mite = mites[0];
    assert!(g.has(mite, Keyword::Toxic(1)));
    g.engine
        .state
        .combat
        .attackers
        .insert(attacker, Target::Player(P0));
    let blockers = mtg_engine::combat::eligible_blockers(&g.engine.state, &g.table, P0);
    assert!(!blockers.iter().any(|(blocker, _)| *blocker == mite));
    let blocks = std::collections::BTreeMap::from([(attacker, vec![mite])]);
    assert!(mtg_engine::combat::validate_blocks(&g.engine.state, &g.table, P0, &blocks).is_err());
    g.engine.state.combat.attackers.clear();
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    g.combat(&[mite], &[], &[], &[]);
    assert_eq!(g.life(P1), 19);
    assert_eq!(g.engine.state.player(P1).poison, 1);
}

#[test]
fn typecycling_filters_reveals_and_shuffles_without_drawing() {
    for (cycling, type_line) in [
        ("Wizardcycling {0}", "Creature — Wizard"),
        ("Artifact landcycling {0}", "Artifact Land"),
    ] {
        let mut t = Table::default();
        let cycler = t.card("{0}", "Creature", Some((2, 2)), cycling);
        let wanted = t.card("{0}", type_line, Some((1, 1)), "");
        let bear = t.bear();
        let artifact = t.card("{0}", "Artifact", None, "");
        let mut g = Game::new(t);
        let source = g.put(cycler, P0, Zone::Hand);
        g.main();
        let wanted_object = g.put(wanted, P0, Zone::Library);
        g.put(bear, P0, Zone::Library);
        g.put(artifact, P0, Zone::Library);
        let hand = g.count(Zone::Hand, P0);
        let log_start = g.engine.log.len();
        g.act_holding(activate(source, 0), &[]);
        let chosen = std::cell::Cell::new(false);
        g.drive(
            |_, choice| {
                if let ChoiceKind::ChooseObjects { from, min, max } = &choice.kind {
                    assert_eq!(from, &vec![wanted_object]);
                    assert_eq!((*min, *max), (0, 1));
                    chosen.set(true);
                    Some(Answer::Objects(vec![wanted_object]))
                } else {
                    None
                }
            },
            |g| chosen.get() && g.stack().is_empty(),
        );
        assert_eq!(g.count(Zone::Hand, P0), hand);
        let in_hand = g.engine.state.objects_in(ZoneRef::of(Zone::Hand, P0));
        assert!(
            in_hand
                .iter()
                .any(|id| g.engine.state.objects[id].card == wanted)
        );
        let in_graveyard = g.engine.state.objects_in(ZoneRef::of(Zone::Graveyard, P0));
        assert!(
            in_graveyard
                .iter()
                .any(|id| g.engine.state.objects[id].card == cycler)
        );
        let events: Vec<_> = g.engine.log[log_start..]
            .iter()
            .map(|entry| &entry.event)
            .collect();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Cycled { .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Revealed { .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Shuffled { .. }))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::Drew { .. }))
        );
    }
}

#[test]
fn kudo_changes_other_creatures_base_size_and_preserves_their_types() {
    let mut t = Table::default();
    let kudo = t.card("{0}", "Creature — Bear", Some((2, 2)), "Other creatures have base power and toughness 2/2 and are Bears in addition to their other types.");
    let elf = t.card("{0}", "Creature — Elf", Some((5, 5)), "");
    let lord = t.card("{0}", "Enchantment", None, "Bears you control get +1/+1.");
    let destroy = t.card("{0}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    let kudo = g.put(kudo, P0, Zone::Battlefield);
    let ours = g.put(elf, P0, Zone::Battlefield);
    let theirs = g.put(elf, P1, Zone::Battlefield);
    g.put(lord, P0, Zone::Battlefield);
    let destroy = g.put(destroy, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(kudo), (3, 3));
    assert_eq!(g.pt(ours), (3, 3));
    assert_eq!(g.pt(theirs), (2, 2));
    let bear = g.table.subtype_named("Bear").unwrap();
    let elf = g.table.subtype_named("Elf").unwrap();
    let ch = g.engine.characteristics(&g.table, ours).unwrap();
    assert!(ch.subtypes.contains(&bear));
    assert!(ch.subtypes.contains(&elf));
    g.cast(destroy, &[Target::Object(kudo)]);
    assert_eq!(g.pt(ours), (5, 5));
    assert_eq!(g.pt(theirs), (5, 5));
    let ch = g.engine.characteristics(&g.table, ours).unwrap();
    assert!(!ch.subtypes.contains(&bear));
    assert!(ch.subtypes.contains(&elf));
}

#[test]
fn water_wings_base_size_and_both_keywords_expire_together() {
    let mut t = Table::default();
    let wings = t.card("{0}", "Instant", None, "Until end of turn, target creature you control has base power and toughness 4/4 and gains flying and hexproof.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let bear = g.put(bear, P0, Zone::Battlefield);
    let wings = g.put(wings, P0, Zone::Hand);
    g.main();
    g.cast(wings, &[Target::Object(bear)]);
    assert_eq!(g.pt(bear), (4, 4));
    assert!(g.has(bear, Keyword::Flying));
    assert!(g.has(bear, Keyword::Hexproof));
    g.until(P1, Step::PrecombatMain);
    assert_eq!(g.pt(bear), (2, 2));
    assert!(!g.has(bear, Keyword::Flying));
    assert!(!g.has(bear, Keyword::Hexproof));
}
