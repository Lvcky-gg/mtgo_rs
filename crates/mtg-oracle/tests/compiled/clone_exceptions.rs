//! Clones whose copy differs: "except it's an artifact in addition to its other types".

use super::harness::*;
use mtg_core::{CardType, Zone};
use mtg_engine::{actions::Action, choice::Answer};

/// The other permanent showing the original's card: a copy takes on what it copies.
fn copy_of(g: &Game, original: mtg_core::ObjectId) -> mtg_core::ObjectId {
    let card = g.engine.state.objects[&original].card;
    *g.engine
        .state
        .battlefield()
        .iter()
        .find(|id| **id != original && g.engine.state.objects[id].card == card)
        .unwrap_or(&original)
}

fn chars(g: &Game, id: mtg_core::ObjectId) -> mtg_core::Characteristics {
    mtg_engine::layers::compute(&g.engine.state, &g.table, id).unwrap()
}

#[test]
fn phyrexian_metamorph_copying_a_creature_is_also_an_artifact() {
    let mut t = Table::default();
    let metamorph = t.card(
        "{3}",
        "Artifact Creature — Phyrexian Shapeshifter",
        Some((0, 0)),
        "You may have this creature enter as a copy of any artifact or creature on the battlefield, except it's an artifact in addition to its other types.",
    );
    let wizard = t.card("{2}{U}", "Creature — Wizard", Some((3, 4)), "");
    let mut g = Game::new(t);
    g.lands(3);
    let original = g.put(wizard, P1, Zone::Battlefield);
    let spell = g.put(metamorph, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![original])],
    );
    let copy = copy_of(&g, original);
    assert_ne!(copy, original, "the Metamorph entered");
    let ch = chars(&g, copy);
    assert!(ch.has_type(CardType::Creature) && ch.has_type(CardType::Artifact));
    assert_eq!(g.pt(copy), (3, 4));
}

#[test]
fn copy_artifact_is_an_enchantment_as_well() {
    let mut t = Table::default();
    let copier = t.card(
        "{1}{U}",
        "Enchantment",
        None,
        "You may have this enchantment enter as a copy of any artifact on the battlefield, except it's an enchantment in addition to its other types.",
    );
    let rock = t.card("{2}", "Artifact", None, "{T}: Add {C}{C}.");
    let mut g = Game::new(t);
    g.lands(2);
    let original = g.put(rock, P0, Zone::Battlefield);
    let spell = g.put(copier, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![original])],
    );
    let copy = copy_of(&g, original);
    let ch = chars(&g, copy);
    assert!(ch.has_type(CardType::Artifact) && ch.has_type(CardType::Enchantment));
}

#[test]
fn phantasmal_image_is_sacrificed_when_targeted() {
    let mut t = Table::default();
    let image = t.card(
        "{1}{U}",
        "Creature — Illusion",
        Some((0, 0)),
        "You may have this creature enter as a copy of any creature on the battlefield, except it's an Illusion in addition to its other types and it has \"When this creature becomes the target of a spell or ability, sacrifice it.\"",
    );
    let giant = t.card("{3}{R}", "Creature — Giant", Some((5, 5)), "");
    let pump = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature gets +1/+1 until end of turn.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let original = g.put(giant, P1, Zone::Battlefield);
    let spell = g.put(image, P0, Zone::Hand);
    let trick = g.put(pump, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![original])],
    );
    let copy = copy_of(&g, original);
    assert_ne!(copy, original);
    assert_eq!(g.pt(copy), (5, 5));
    g.act(
        Action::Cast { object: trick },
        &[mtg_core::Target::Object(copy)],
        &[],
    );
    assert!(
        !g.engine.state.objects.contains_key(&copy),
        "the Image was sacrificed"
    );
    assert!(g.engine.state.objects.contains_key(&original));
}
