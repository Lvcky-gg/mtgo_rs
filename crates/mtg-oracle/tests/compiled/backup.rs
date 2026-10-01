//! Backup (CR 702.165): a +1/+1 counter, and the abilities below it, handed on for a turn.
use super::harness::*;
use mtg_core::{Keyword, Step, Target, Zone};

fn skald(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{1}{R}",
        "Creature — Dwarf Berserker",
        Some((1, 1)),
        "Backup 1 (When this creature enters, put a +1/+1 counter on target creature. If that's \
         another creature, it gains the following ability until end of turn.)\nFlying",
    )
}

#[test]
fn backup_hands_its_abilities_to_another_creature_for_the_turn() {
    let mut t = Table::default();
    let skald = skald(&mut t);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(skald, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(b)]);
    assert_eq!(g.pt(b), (3, 3), "the counter");
    assert!(g.has(b, Keyword::Flying), "the following ability");
    let s = g.find(skald).expect("on the battlefield");
    assert_eq!(g.pt(s), (1, 1));
    assert!(g.has(s, Keyword::Flying), "it keeps its own");
    g.until(P1, Step::Upkeep);
    assert_eq!(g.pt(b), (3, 3), "the counter stays");
    assert!(
        !g.has(b, Keyword::Flying),
        "the ability was until end of turn"
    );
}
