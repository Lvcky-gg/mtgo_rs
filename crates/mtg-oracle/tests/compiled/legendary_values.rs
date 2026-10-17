use super::harness::*;
use mtg_core::{AbilityId, CardType, Color, Keyword, Step, Target, Zone};
use mtg_engine::actions::Action;

#[test]
fn chosen_color_mana_counts_distinct_current_powers_including_zero_and_negative() {
    let mut t = Table::default();
    let loot = t.named_card("Loot, the Nexus", "{1}{G}{U}", "Legendary Creature — Beast Noble", Some((1, 4)), "Vigilance\n{T}: Choose a color. Add one mana of that color for each different power among creatures you control.");
    let two = t.bear();
    let zero = t.card("{0}", "Creature — Wall", Some((0, 3)), "");
    let negative = t.card("{0}", "Creature — Beast", Some((-1, 3)), "");
    let large = t.card("{0}", "Creature — Beast", Some((7, 7)), "");
    let pump = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature gets +1/+0 until end of turn.",
    );
    let mut g = Game::new(t);
    let loot = g.put(loot, P0, Zone::Battlefield);
    let duplicate = g.put(two, P0, Zone::Battlefield);
    g.put(two, P0, Zone::Battlefield);
    g.put(two, P0, Zone::Battlefield);
    g.put(zero, P0, Zone::Battlefield);
    g.put(negative, P0, Zone::Battlefield);
    g.put(large, P1, Zone::Battlefield);
    let pump = g.put(pump, P0, Zone::Hand);
    g.main();
    g.cast(pump, &[Target::Object(duplicate)]);
    g.act(
        Action::ActivateManaAbility {
            source: loot,
            ability: AbilityId(1),
            color: Some(Color::Blue),
        },
        &[],
        &[],
    );
    // Loot's 1, the Bears' 2 and 3 (two still share 2), zero, and -1.
    // The opponent's 7 is excluded.
    assert_eq!(
        g.engine.state.player(P0).mana.amounts[Color::Blue as usize],
        5
    );
    assert!(g.stack().is_empty());
}

#[test]
fn distinct_power_mana_is_available_to_the_automatic_payment_planner() {
    let mut t = Table::default();
    let source = t.card("{0}", "Creature — Elf", Some((1, 3)), "{T}: Choose a color. Add one mana of that color for each different power among creatures you control.");
    let bear = t.bear();
    let spell = t.card("{U}{U}", "Sorcery", None, "You gain 3 life.");
    let mut g = Game::new(t);
    g.put(source, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    assert!(g.main().contains(&Action::Cast { object: spell }));
    g.cast(spell, &[]);
    assert_eq!(g.life(P0), 23);
}

#[test]
fn mana_value_animation_preserves_color_and_abilities_and_expires() {
    let mut t = Table::default();
    let karn = t.named_card("Karn, Silver Golem", "{5}", "Legendary Artifact Creature — Golem", Some((4, 4)), "Whenever Karn blocks or becomes blocked, it gets -4/+4 until end of turn.\n{1}: Target noncreature artifact becomes an artifact creature with power and toughness each equal to its mana value until end of turn.");
    let artifact = t.card("{2}{U}", "Artifact", None, "Vigilance\n{T}: Add {G}.");
    let mut g = Game::new(t);
    g.lands(1);
    let karn = g.put(karn, P0, Zone::Battlefield);
    let artifact = g.put(artifact, P0, Zone::Battlefield);
    g.main();
    g.act(activate(karn, 1), &[Target::Object(artifact)], &[]);
    assert_eq!(g.pt(artifact), (3, 3));
    let ch = g.engine.characteristics(&g.table, artifact).unwrap();
    assert!(ch.card_types.contains(&CardType::Artifact));
    assert!(ch.card_types.contains(&CardType::Creature));
    assert!(ch.colors.contains(Color::Blue));
    assert!(g.has(artifact, Keyword::Vigilance));
    g.until(P1, Step::Upkeep);
    let ch = g.engine.characteristics(&g.table, artifact).unwrap();
    assert!(!ch.card_types.contains(&CardType::Creature));
    assert_eq!(ch.power, None);
}

#[test]
fn animating_a_zero_mana_value_artifact_puts_it_into_the_graveyard() {
    let mut t = Table::default();
    let animator = t.card("{0}", "Creature — Wizard", Some((1, 3)), "{0}: Target noncreature artifact becomes an artifact creature with power and toughness each equal to its mana value until end of turn.");
    let artifact = t.card("{0}", "Artifact", None, "");
    let mut g = Game::new(t);
    let animator = g.put(animator, P0, Zone::Battlefield);
    let artifact_id = g.put(artifact, P0, Zone::Battlefield);
    g.main();
    g.act(activate(animator, 0), &[Target::Object(artifact_id)], &[]);
    assert!(
        g.engine
            .state
            .objects
            .values()
            .any(|o| o.card == artifact && o.zone.zone == Zone::Graveyard)
    );
}

#[test]
fn gendered_damage_and_counter_pronouns_follow_the_triggering_creature() {
    let mut t = Table::default();
    let wasp = t.named_card("The Wasp, Janet Van Dyne", "{2}{W}", "Legendary Creature — Human Hero", Some((1, 1)), "Flying\nWhen The Wasp enters, she deals 4 damage to target tapped creature an opponent controls.");
    let momo = t.named_card("Momo, Rambunctious Rascal", "{2}{W}", "Legendary Creature — Lemur Bat Ally", Some((1, 1)), "Flying\nWhen Momo enters, he deals 4 damage to target tapped creature an opponent controls.");
    let zuko = t.named_card("Zuko, Seeking Honor", "{2}{B/R}", "Legendary Creature — Human Noble", Some((2, 2)), "Firebending 1\nWhenever you cast a noncreature spell, Zuko gains first strike until end of turn.\nWhenever Zuko deals combat damage to a player, put a +1/+1 counter on him.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(6);
    let zuko = g.put(zuko, P0, Zone::Battlefield);
    let bear1 = g.put(bear, P1, Zone::Battlefield);
    let bear2 = g.put(bear, P1, Zone::Battlefield);
    for id in [bear1, bear2] {
        g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
    }
    let wasp = g.put(wasp, P0, Zone::Hand);
    let momo = g.put(momo, P0, Zone::Hand);
    g.main();
    g.cast(wasp, &[Target::Object(bear1)]);
    g.cast(momo, &[Target::Object(bear2)]);
    assert!(!g.engine.state.objects.contains_key(&bear1));
    assert!(!g.engine.state.objects.contains_key(&bear2));
    g.combat(&[zuko], &[], &[], &[]);
    assert_eq!(g.pt(zuko), (3, 3));
}

#[test]
fn her_refers_to_the_previous_target_rather_than_the_ability_source() {
    let mut t = Table::default();
    let source = t.card(
        "{0}",
        "Creature — Wizard",
        Some((1, 3)),
        "{0}: Target creature gets +1/+1 until end of turn. Untap her.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let source = g.put(source, P0, Zone::Battlefield);
    let bear = g.put(bear, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&bear).unwrap().tapped = true;
    g.main();
    g.act(activate(source, 0), &[Target::Object(bear)], &[]);
    assert!(!g.engine.state.objects[&bear].tapped);
    assert_eq!(g.pt(bear), (3, 3));
    assert_eq!(g.pt(source), (1, 3));
}

#[test]
fn a_lesson_in_your_graveyard_dynamically_grants_aangs_vigilance() {
    let mut t = Table::default();
    let lesson = t.card("{0}", "Sorcery — Lesson", None, "You gain 1 life.");
    let aang = t.named_card("Aang, A Lot to Learn", "{2}{G/W}", "Legendary Creature — Human Avatar Ally", Some((3, 2)), "Aang has vigilance as long as there's a Lesson card in your graveyard.\nWhenever another creature you control dies, put a +1/+1 counter on Aang.");
    let exile = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target card from a graveyard.",
    );
    let lesson_card = lesson;
    let mut g = Game::new(t);
    let aang = g.put(aang, P0, Zone::Battlefield);
    g.put(lesson, P1, Zone::Graveyard);
    let lesson = g.put(lesson, P0, Zone::Hand);
    let exile = g.put(exile, P0, Zone::Hand);
    g.main();
    assert!(!g.has(aang, Keyword::Vigilance));
    g.cast(lesson, &[]);
    assert!(g.has(aang, Keyword::Vigilance));
    let discarded = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == lesson_card && o.owner == P0 && o.zone.zone == Zone::Graveyard)
        .unwrap()
        .id;
    g.cast(exile, &[Target::Object(discarded)]);
    assert!(!g.has(aang, Keyword::Vigilance));
}

#[test]
fn alirios_only_stays_tapped_while_you_control_a_reflection() {
    let mut t = Table::default();
    let reflection = t.card("{0}", "Creature — Reflection", Some((3, 2)), "");
    let alirios = t.named_card("Alirios, Enraptured", "{2}{U}", "Legendary Creature — Human", Some((2, 3)), "Alirios enters tapped.\nAlirios doesn't untap during your untap step if you control a Reflection.\nWhen Alirios enters, create a 3/2 blue Reflection creature token.");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let alirios = g.put(alirios, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&alirios).unwrap().tapped = true;
    let yours = g.put(reflection, P0, Zone::Battlefield);
    g.put(reflection, P1, Zone::Battlefield);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.until(P1, Step::Upkeep);
    g.main();
    assert!(g.engine.state.objects[&alirios].tapped);
    g.cast(bounce, &[Target::Object(yours)]);
    g.until(P1, Step::Upkeep);
    g.main();
    assert!(!g.engine.state.objects[&alirios].tapped);
}

#[test]
fn traxos_enters_tapped_and_stays_tapped_until_a_historic_spell_is_cast() {
    let mut t = Table::default();
    let traxos = t.named_card("Traxos, Scourge of Kroog", "{4}", "Legendary Artifact Creature — Dragon Construct", Some((7, 7)), "Trample\nTraxos enters tapped and doesn't untap during your untap step.\nWhenever you cast a historic spell, untap Traxos.");
    let artifact = t.card("{0}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(4);
    let hand = g.put(traxos, P0, Zone::Hand);
    let artifact = g.put(artifact, P0, Zone::Hand);
    g.main();
    g.cast(hand, &[]);
    let traxos = g.find(traxos).unwrap();
    assert!(g.engine.state.objects[&traxos].tapped);
    g.until(P1, Step::Upkeep);
    g.main();
    assert!(g.engine.state.objects[&traxos].tapped);
    g.cast(artifact, &[]);
    assert!(!g.engine.state.objects[&traxos].tapped);
}
