//! Changeling (CR 702.73): every creature type, in every zone.
use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

fn mimic(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{1}{R}",
        "Creature — Shapeshifter",
        Some((1, 1)),
        "Changeling",
    )
}

#[test]
fn a_changeling_gets_every_tribal_bonus() {
    let mut t = Table::default();
    let goblin_lord = t.card(
        "{1}{R}",
        "Creature — Goblin",
        Some((2, 2)),
        "Other Goblins you control get +1/+1.",
    );
    let elf_lord = t.card(
        "{1}{G}",
        "Creature — Elf",
        Some((2, 2)),
        "Other Elves you control get +1/+1.",
    );
    let mimic = mimic(&mut t);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(goblin_lord, P0, Zone::Battlefield);
    g.put(elf_lord, P0, Zone::Battlefield);
    let m = g.put(mimic, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    assert_eq!(g.pt(m), (3, 3), "a Goblin and an Elf");
    assert_eq!(g.pt(b), (2, 2), "a Bear is neither");
}

#[test]
fn a_changeling_is_every_creature_type_while_it_is_a_spell() {
    let mut t = Table::default();
    let warchief = t.card(
        "{1}{R}",
        "Creature — Goblin",
        Some((2, 2)),
        "Goblin spells you cast cost {1} less to cast.",
    );
    let chronicler = t.card(
        "{2}{R}",
        "Creature — Goblin",
        Some((1, 1)),
        "Whenever you cast a Goblin spell, draw a card.",
    );
    let mimic = mimic(&mut t);
    let mut g = Game::new(t);
    g.lands(1);
    g.put(warchief, P0, Zone::Battlefield);
    g.put(chronicler, P0, Zone::Battlefield);
    let m = g.put(mimic, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        actions.contains(&Action::Cast { object: m }),
        "{{1}}{{R}} for one land: the Goblin discount applies in the hand"
    );
    let hand = g.count(Zone::Hand, P0);
    g.cast(m, &[]);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand,
        "cast a Goblin spell (-1), drew a card (+1)"
    );
    assert!(g.find(mimic).is_some());
}
