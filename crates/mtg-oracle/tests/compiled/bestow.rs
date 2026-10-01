//! Bestow (CR 702.103): an Aura while it enchants something, a creature otherwise.
use super::harness::*;
use mtg_core::{CardType, Target, Zone};
use mtg_engine::actions::Action;

fn satyr(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{1}{G}",
        "Enchantment Creature — Satyr",
        Some((2, 1)),
        "Bestow {2}{G} (If you cast this card for its bestow cost, it's an Aura spell with \
         enchant creature. It becomes a creature again if it's not attached to a creature.)\n\
         Enchanted creature gets +2/+1.",
    )
}

fn bestow(actions: &[Action], object: mtg_core::ObjectId) -> Action {
    actions
        .iter()
        .find(|a| matches!(a, Action::CastAlternative { object: o, .. } if *o == object))
        .cloned()
        .expect("bestow is offered")
}

#[test]
fn a_bestowed_aura_falls_off_as_a_creature() {
    let mut t = Table::default();
    let satyr = satyr(&mut t);
    let bear = t.bear();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(4);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(satyr, P0, Zone::Hand);
    let bolt = g.put(bolt, P0, Zone::Hand);
    let actions = g.main();
    g.act(bestow(&actions, s), &[Target::Object(b)], &[]);
    let s = g.find(satyr).expect("on the battlefield");
    assert_eq!(g.engine.state.objects[&s].attached_to, Some(b));
    assert_eq!(g.pt(b), (4, 3), "enchanted");
    let ch = g.engine.characteristics(&g.table, s).unwrap().clone();
    assert!(!ch.has_type(CardType::Creature), "an Aura, not a creature");
    assert!(ch.has_type(CardType::Enchantment));

    g.cast(bolt, &[Target::Object(b)]);
    assert!(!g.engine.state.objects.contains_key(&b), "the bear died");
    let ch = g.engine.characteristics(&g.table, s).unwrap().clone();
    assert!(ch.has_type(CardType::Creature), "a creature again");
    assert_eq!(g.engine.state.objects[&s].attached_to, None);
    assert_eq!(g.pt(s), (2, 1));
}

#[test]
fn a_bestow_spell_without_its_target_resolves_as_a_creature() {
    let mut t = Table::default();
    let satyr = satyr(&mut t);
    let bear = t.bear();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(4);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(satyr, P0, Zone::Hand);
    let bolt = g.put(bolt, P0, Zone::Hand);
    let actions = g.main();
    g.act_holding(bestow(&actions, s), &[Target::Object(b)]);
    g.cast(bolt, &[Target::Object(b)]);
    let s = g.find(satyr).expect("resolved anyway");
    assert_eq!(g.engine.state.objects[&s].zone.zone, Zone::Battlefield);
    let ch = g.engine.characteristics(&g.table, s).unwrap().clone();
    assert!(ch.has_type(CardType::Creature));
    assert_eq!(g.pt(s), (2, 1));
}
