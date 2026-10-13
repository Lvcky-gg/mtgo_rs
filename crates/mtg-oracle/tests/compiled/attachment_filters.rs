use super::harness::*;
use mtg_core::{Keyword, Target, Zone};

#[test]
fn enchanted_creature_anthem_distinguishes_auras_from_equipment() {
    let mut t = Table::default();
    let tale = t.card(
        "{0}",
        "Enchantment",
        None,
        "Enchanted creatures you control get +2/+2.",
    );
    let bear = t.bear();
    let aura = t.card("{0}", "Enchantment — Aura", None, "Enchant creature");
    let equipment = t.card("{0}", "Artifact — Equipment", None, "Equip {0}");
    let destroy = t.card("{0}", "Instant", None, "Destroy target enchantment.");
    let mut g = Game::new(t);
    g.put(tale, P0, Zone::Battlefield);
    let enchanted = g.put(bear, P0, Zone::Battlefield);
    let equipped = g.put(bear, P0, Zone::Battlefield);
    let opposing = g.put(bear, P1, Zone::Battlefield);
    let a = g.put(aura, P0, Zone::Battlefield);
    let b = g.put(equipment, P0, Zone::Battlefield);
    let c = g.put(aura, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&a).unwrap().attached_to = Some(enchanted);
    g.engine.state.objects.get_mut(&b).unwrap().attached_to = Some(equipped);
    g.engine.state.objects.get_mut(&c).unwrap().attached_to = Some(opposing);
    let destroy = g.put(destroy, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(enchanted), (4, 4));
    assert_eq!(g.pt(equipped), (2, 2));
    assert_eq!(g.pt(opposing), (2, 2));
    g.cast(destroy, &[Target::Object(a)]);
    assert_eq!(
        g.pt(enchanted),
        (2, 2),
        "a departed Aura no longer enchants"
    );
}

#[test]
fn greater_auramancy_protects_enchantments_and_only_enchanted_creatures() {
    let mut t = Table::default();
    let auramancy = t.card(
        "{0}",
        "Enchantment",
        None,
        "Other enchantments you control have shroud.\nEnchanted creatures you control have shroud.",
    );
    let bear = t.bear();
    let aura = t.card("{0}", "Enchantment — Aura", None, "Enchant creature");
    let mut g = Game::new(t);
    let shield = g.put(auramancy, P0, Zone::Battlefield);
    let enchanted = g.put(bear, P0, Zone::Battlefield);
    let bare = g.put(bear, P0, Zone::Battlefield);
    let a = g.put(aura, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&a).unwrap().attached_to = Some(enchanted);
    assert!(g.has(a, Keyword::Shroud));
    assert!(g.has(enchanted, Keyword::Shroud));
    assert!(!g.has(shield, Keyword::Shroud));
    assert!(!g.has(bare, Keyword::Shroud));
}

#[test]
fn stone_haven_outfitter_buffs_equipped_creatures_and_draws_when_they_die() {
    let mut t = Table::default();
    let outfitter = t.card("{0}", "Creature", Some((2, 2)), "Equipped creatures you control get +1/+1.\nWhenever an equipped creature you control dies, draw a card.");
    let bear = t.bear();
    let equipment = t.card("{0}", "Artifact — Equipment", None, "Equip {0}");
    let destroy = t.card("{0}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    g.put(outfitter, P0, Zone::Battlefield);
    let equipped = g.put(bear, P0, Zone::Battlefield);
    let bare = g.put(bear, P0, Zone::Battlefield);
    let e = g.put(equipment, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&e).unwrap().attached_to = Some(equipped);
    let first = g.put(destroy, P0, Zone::Hand);
    let second = g.put(destroy, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(equipped), (3, 3));
    assert_eq!(g.pt(bare), (2, 2));
    let before = g.count(Zone::Hand, P0);
    g.cast(first, &[Target::Object(equipped)]);
    assert_eq!(g.count(Zone::Hand, P0), before);
    g.cast(second, &[Target::Object(bare)]);
    assert_eq!(g.count(Zone::Hand, P0), before - 1);
}

#[test]
fn simultaneous_death_keeps_attachment_information_but_an_earlier_death_does_not() {
    for earlier in [false, true] {
        let mut t = Table::default();
        let watch = t.card(
            "{0}",
            "Enchantment",
            None,
            "Whenever an equipped creature you control dies, draw a card.",
        );
        let equipment = t.card(
            "{0}",
            "Artifact Creature — Equipment",
            Some((1, 1)),
            "Equip {0}",
        );
        let bear = t.bear();
        let artifact_removal = t.card("{0}", "Instant", None, "Destroy target artifact.");
        let wrath = t.card("{0}", "Sorcery", None, "Destroy all creatures.");
        let mut g = Game::new(t);
        g.put(watch, P0, Zone::Battlefield);
        // The attachment is applied first in the simultaneous destruction batch.
        let e = g.put(equipment, P0, Zone::Battlefield);
        let body = g.put(bear, P0, Zone::Battlefield);
        g.engine.state.objects.get_mut(&e).unwrap().attached_to = Some(body);
        let remove = g.put(artifact_removal, P0, Zone::Hand);
        let wrath = g.put(wrath, P0, Zone::Hand);
        g.main();
        if earlier {
            g.cast(remove, &[Target::Object(e)]);
        }
        let before = g.count(Zone::Hand, P0);
        g.cast(wrath, &[]);
        assert_eq!(g.count(Zone::Hand, P0), before - usize::from(earlier));
    }
}
