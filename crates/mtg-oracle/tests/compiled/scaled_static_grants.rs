use super::harness::*;
use mtg_core::{Keyword, Target, Zone};

#[test]
fn ethereal_armor_counts_enchantments_and_grants_first_strike_only_to_its_host() {
    let mut t = Table::default();
    let bear = t.bear();
    let armor = t.card("{0}", "Enchantment — Aura", None, "Enchant creature\nEnchanted creature gets +1/+1 for each enchantment you control and has first strike.");
    let enchantment = t.card("{0}", "Enchantment", None, "");
    let destroy = t.card("{0}", "Instant", None, "Destroy target enchantment.");
    let mut g = Game::new(t);
    let ours = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    g.put(enchantment, P0, Zone::Battlefield);
    g.put(enchantment, P1, Zone::Battlefield);
    let armor = g.put(armor, P0, Zone::Hand);
    let destroy = g.put(destroy, P0, Zone::Hand);
    g.main();
    g.cast(armor, &[Target::Object(ours)]);
    assert_eq!(g.pt(ours), (4, 4));
    assert!(g.has(ours, Keyword::FirstStrike));
    assert_eq!(g.pt(theirs), (2, 2));
    assert!(!g.has(theirs, Keyword::FirstStrike));
    let armor = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].attached_to == Some(ours))
        .unwrap();
    g.cast(destroy, &[Target::Object(armor)]);
    assert_eq!(g.pt(ours), (2, 2));
    assert!(!g.has(ours, Keyword::FirstStrike));
}

#[test]
fn hold_the_gates_applies_the_counted_bonus_and_vigilance_to_your_creatures() {
    let mut t = Table::default();
    let bear = t.bear();
    let gate = t.card("", "Land — Gate", None, "");
    let hold = t.card(
        "{0}",
        "Enchantment",
        None,
        "Creatures you control get +0/+1 for each Gate you control and have vigilance.",
    );
    let mut g = Game::new(t);
    let ours = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    g.put(gate, P0, Zone::Battlefield);
    g.put(gate, P0, Zone::Battlefield);
    g.put(gate, P1, Zone::Battlefield);
    g.put(hold, P0, Zone::Battlefield);
    g.main();
    assert_eq!(g.pt(ours), (2, 4));
    assert!(g.has(ours, Keyword::Vigilance));
    assert_eq!(g.pt(theirs), (2, 2));
    assert!(!g.has(theirs, Keyword::Vigilance));
    g.combat(&[ours], &[], &[], &[]);
    assert!(!g.engine.state.objects[&ours].tapped);
    assert_eq!(g.life(P1), 18);
}

#[test]
fn auramancers_guise_counts_auras_on_the_enchanted_creature() {
    let mut t = Table::default();
    let bear = t.bear();
    let aura = t.card("{0}", "Enchantment — Aura", None, "Enchant creature");
    let guise = t.card("{0}", "Enchantment — Aura", None, "Enchant creature\nEnchanted creature gets +2/+2 for each Aura attached to it and has vigilance.");
    let mut g = Game::new(t);
    let host = g.put(bear, P0, Zone::Battlefield);
    let other = g.put(bear, P1, Zone::Battlefield);
    for attached_to in [host, other] {
        let aura = g.put(aura, P0, Zone::Battlefield);
        g.engine.state.objects.get_mut(&aura).unwrap().attached_to = Some(attached_to);
    }
    let guise = g.put(guise, P0, Zone::Hand);
    g.main();
    g.cast(guise, &[Target::Object(host)]);
    assert_eq!(g.pt(host), (6, 6));
    assert!(g.has(host, Keyword::Vigilance));
    assert_eq!(g.pt(other), (2, 2));
}

#[test]
fn champion_of_the_flame_counts_attached_auras_and_equipment_once_each() {
    let mut t = Table::default();
    let champion = t.card(
        "{0}",
        "Creature",
        Some((2, 2)),
        "This creature gets +2/+2 for each Aura and Equipment attached to it.",
    );
    let bear = t.bear();
    let aura = t.card("{0}", "Enchantment — Aura", None, "Enchant creature");
    let equipment = t.card("{0}", "Artifact — Equipment", None, "Equip {0}");
    let both = t.card(
        "{0}",
        "Artifact Enchantment — Aura Equipment",
        None,
        "Enchant creature",
    );
    let mut g = Game::new(t);
    let champion = g.put(champion, P0, Zone::Battlefield);
    let other = g.put(bear, P0, Zone::Battlefield);
    for (card, owner, host) in [
        (aura, P1, champion),
        (equipment, P0, champion),
        (both, P0, champion),
        (aura, P0, other),
    ] {
        let attachment = g.put(card, owner, Zone::Battlefield);
        g.engine
            .state
            .objects
            .get_mut(&attachment)
            .unwrap()
            .attached_to = Some(host);
    }
    g.main();
    assert_eq!(g.pt(champion), (8, 8));
    assert_eq!(g.pt(other), (2, 2));
}

#[test]
fn wyleth_draws_for_both_attachment_types_but_not_other_creatures_attachments() {
    let mut t = Table::default();
    let wyleth = t.card(
        "{0}",
        "Creature",
        Some((2, 2)),
        "Whenever this creature attacks, draw a card for each Aura and Equipment attached to it.",
    );
    let bear = t.bear();
    let aura = t.card("{0}", "Enchantment — Aura", None, "Enchant creature");
    let equipment = t.card("{0}", "Artifact — Equipment", None, "Equip {0}");
    let mut g = Game::new(t);
    let wyleth = g.put(wyleth, P0, Zone::Battlefield);
    let other = g.put(bear, P0, Zone::Battlefield);
    for (card, host) in [(aura, wyleth), (equipment, wyleth), (aura, other)] {
        let attachment = g.put(card, P0, Zone::Battlefield);
        g.engine
            .state
            .objects
            .get_mut(&attachment)
            .unwrap()
            .attached_to = Some(host);
    }
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.combat(&[wyleth], &[], &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand + 2);
}

#[test]
fn toil_to_renown_counts_each_tapped_matching_permanent_once() {
    let mut t = Table::default();
    let artifact_creature = t.card("{0}", "Artifact Creature", Some((2, 2)), "");
    let land = t.card("", "Land", None, "");
    let toil = t.card(
        "{0}",
        "Sorcery",
        None,
        "You gain 1 life for each tapped artifact, creature, and land you control.",
    );
    let mut g = Game::new(t);
    let ours = [
        g.put(artifact_creature, P0, Zone::Battlefield),
        g.put(land, P0, Zone::Battlefield),
    ];
    g.put(artifact_creature, P0, Zone::Battlefield);
    let opponent = g.put(artifact_creature, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&opponent).unwrap().tapped = true;
    let toil = g.put(toil, P0, Zone::Hand);
    g.main();
    for object in ours {
        mtg_engine::apply::apply(
            &mut g.engine.state,
            mtg_core::Cause::StateBasedAction,
            mtg_core::Event::TapChanged {
                object,
                tapped: true,
            },
            &mut g.engine.log,
        );
    }
    g.cast(toil, &[]);
    assert_eq!(g.life(P0), 22);
}
