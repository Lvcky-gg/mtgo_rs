use super::harness::*;
use mtg_core::{CardType, Color, Keyword, Step, Target, Zone, ZoneRef};
use mtg_ir::PrintedCards;

#[test]
fn ensoul_artifact_preserves_artifact_type_and_mana_ability_and_ends_when_removed() {
    let mut t = Table::default();
    let ensoul = t.card("{0}", "Enchantment — Aura", None, "Enchant artifact\nEnchanted artifact is a creature with base power and toughness 5/5 in addition to its other types.");
    let rock = t.card("{0}", "Artifact", None, "{T}: Add {C}.");
    let destroy = t.card("{0}", "Instant", None, "Destroy target enchantment.");
    let mut g = Game::new(t);
    let rock = g.put(rock, P0, Zone::Battlefield);
    let spell = g.put(ensoul, P0, Zone::Hand);
    let destroy = g.put(destroy, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(rock)]);
    assert_eq!(g.pt(rock), (5, 5));
    let ch = g.engine.characteristics(&g.table, rock).unwrap();
    assert!(ch.has_type(CardType::Artifact));
    assert!(ch.has_type(CardType::Creature));
    let mana = mtg_engine::actions::Action::ActivateManaAbility {
        source: rock,
        ability: mtg_core::AbilityId(0),
        color: None,
    };
    g.act(mana, &[], &[]);
    assert_eq!(g.engine.state.player(P0).mana.total(), 1);
    let aura = g.find(ensoul).unwrap();
    g.cast(destroy, &[Target::Object(aura)]);
    let ch = g.engine.characteristics(&g.table, rock).unwrap();
    assert!(ch.has_type(CardType::Artifact));
    assert!(!ch.has_type(CardType::Creature));
    assert_eq!(ch.power, None);
    assert_eq!(ch.toughness, None);
}

#[test]
fn ghoulflesh_adds_black_and_zombie_while_retaining_red_and_elf() {
    let mut t = Table::default();
    let flesh = t.card("{0}", "Enchantment — Aura", None, "Enchant creature\nEnchanted creature gets -1/-1 and is a black Zombie in addition to its other colors and types.");
    let elf = t.card("{R}", "Creature — Elf", Some((3, 3)), "");
    let destroy = t.card("{0}", "Instant", None, "Destroy target enchantment.");
    let mut g = Game::new(t);
    let elf = g.put(elf, P1, Zone::Battlefield);
    let flesh_spell = g.put(flesh, P0, Zone::Hand);
    let destroy = g.put(destroy, P0, Zone::Hand);
    let elf_type = g.table.subtype_named("Elf").unwrap();
    let zombie = g.table.subtype_named("Zombie").unwrap();
    g.main();
    g.cast(flesh_spell, &[Target::Object(elf)]);
    assert_eq!(g.pt(elf), (2, 2));
    let ch = g.engine.characteristics(&g.table, elf).unwrap();
    assert!(ch.colors.contains(Color::Red));
    assert!(ch.colors.contains(Color::Black));
    assert!(ch.subtypes.contains(&elf_type));
    assert!(ch.subtypes.contains(&zombie));
    let flesh = g.find(flesh).unwrap();
    g.cast(destroy, &[Target::Object(flesh)]);
    assert_eq!(g.pt(elf), (3, 3));
    let ch = g.engine.characteristics(&g.table, elf).unwrap();
    assert!(ch.colors.contains(Color::Red));
    assert!(!ch.colors.contains(Color::Black));
    assert!(ch.subtypes.contains(&elf_type));
    assert!(!ch.subtypes.contains(&zombie));
}

#[test]
fn indigo_faerie_adds_blue_temporarily_without_replacing_existing_colors() {
    for cost in ["{R}", "{U}{R}", "{0}"] {
        let mut t = Table::default();
        let faerie = t.card("{0}", "Creature", Some((1, 1)), "Flying\n{0}: Target permanent becomes blue in addition to its other colors until end of turn.");
        let permanent = t.card(cost, "Artifact", None, "");
        let mut g = Game::new(t);
        let source = g.put(faerie, P0, Zone::Battlefield);
        let target = g.put(permanent, P1, Zone::Battlefield);
        g.main();
        let original = g.engine.characteristics(&g.table, target).unwrap().colors;
        g.act(activate(source, 1), &[Target::Object(target)], &[]);
        let colors = g.engine.characteristics(&g.table, target).unwrap().colors;
        assert_eq!(
            colors,
            original.union(mtg_core::ColorSet::single(Color::Blue))
        );
        g.until(P1, Step::PrecombatMain);
        assert_eq!(
            g.engine.characteristics(&g.table, target).unwrap().colors,
            original
        );
    }
}

#[test]
fn rise_from_the_grave_modifies_the_returned_identity_and_resets_on_a_later_zone_change() {
    let mut t = Table::default();
    let rise = t.card("{0}", "Sorcery", None, "Put target creature card from a graveyard onto the battlefield under your control. That creature is a black Zombie in addition to its other colors and types.");
    let elf = t.card("{R}", "Creature — Elf", Some((2, 2)), "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let dead = g.put(elf, P1, Zone::Graveyard);
    let spell = g.put(rise, P0, Zone::Hand);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(dead)]);
    let returned = g.find(elf).unwrap();
    assert_ne!(returned, dead);
    assert_eq!(g.engine.state.objects[&returned].owner, P1);
    assert_eq!(
        mtg_engine::layers::controller(&g.engine.state, returned),
        Some(P0)
    );
    let zombie = g.table.subtype_named("Zombie").unwrap();
    let elf_type = g.table.subtype_named("Elf").unwrap();
    let ch = g.engine.characteristics(&g.table, returned).unwrap();
    assert!(ch.colors.contains(Color::Red));
    assert!(ch.colors.contains(Color::Black));
    assert!(ch.subtypes.contains(&elf_type));
    assert!(ch.subtypes.contains(&zombie));
    g.cast(bounce, &[Target::Object(returned)]);
    let hand = g.engine.state.objects_in(ZoneRef::of(Zone::Hand, P1));
    let moved = hand
        .into_iter()
        .find(|id| g.engine.state.objects[id].card == elf)
        .unwrap();
    let ch = g.engine.characteristics(&g.table, moved).unwrap();
    assert!(ch.colors.contains(Color::Red));
    assert!(!ch.colors.contains(Color::Black));
    assert!(ch.subtypes.contains(&elf_type));
    assert!(!ch.subtypes.contains(&zombie));
}

#[test]
fn hivestone_adds_sliver_to_only_its_controllers_creatures_and_enables_tribal_bonuses() {
    let mut t = Table::default();
    let hive = t.card(
        "{0}",
        "Artifact",
        None,
        "Creatures you control are Slivers in addition to their other creature types.",
    );
    let lord = t.card("{0}", "Enchantment", None, "Slivers you control get +1/+1.");
    let elf = t.card("{0}", "Creature — Elf", Some((2, 2)), "");
    let destroy = t.card("{0}", "Instant", None, "Destroy target artifact.");
    let mut g = Game::new(t);
    let hive = g.put(hive, P0, Zone::Battlefield);
    g.put(lord, P0, Zone::Battlefield);
    let ours = g.put(elf, P0, Zone::Battlefield);
    let theirs = g.put(elf, P1, Zone::Battlefield);
    let destroy = g.put(destroy, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(ours), (3, 3));
    assert_eq!(g.pt(theirs), (2, 2));
    g.cast(destroy, &[Target::Object(hive)]);
    assert_eq!(g.pt(ours), (2, 2));
}

#[test]
fn angelic_armaments_combines_a_pump_flying_and_additive_color_and_type() {
    let mut t = Table::default();
    let armaments = t.card("{0}", "Artifact — Equipment", None, "Equipped creature gets +2/+2, has flying, and is a white Angel in addition to its other colors and types.\nEquip {0}");
    let elf = t.card("{G}", "Creature — Elf", Some((2, 2)), "");
    let destroy = t.card("{0}", "Instant", None, "Destroy target artifact.");
    let mut g = Game::new(t);
    let equipment = g.put(armaments, P0, Zone::Battlefield);
    let elf = g.put(elf, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&equipment)
        .unwrap()
        .attached_to = Some(elf);
    let destroy = g.put(destroy, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(elf), (4, 4));
    assert!(g.has(elf, Keyword::Flying));
    let angel = g.table.subtype_named("Angel").unwrap();
    let elf_type = g.table.subtype_named("Elf").unwrap();
    let ch = g.engine.characteristics(&g.table, elf).unwrap();
    assert!(ch.colors.contains(Color::Green));
    assert!(ch.colors.contains(Color::White));
    assert!(ch.subtypes.contains(&angel));
    assert!(ch.subtypes.contains(&elf_type));
    g.cast(destroy, &[Target::Object(equipment)]);
    assert_eq!(g.pt(elf), (2, 2));
    assert!(!g.has(elf, Keyword::Flying));
    let ch = g.engine.characteristics(&g.table, elf).unwrap();
    assert!(ch.colors.contains(Color::Green));
    assert!(!ch.colors.contains(Color::White));
    assert!(!ch.subtypes.contains(&angel));
}
