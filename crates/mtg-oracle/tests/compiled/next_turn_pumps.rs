use super::harness::*;
use mtg_core::{Keyword, Step, Target, Zone};

#[test]
fn pump_survives_opponents_turn_and_expires_at_controllers_next_turn() {
    let mut t = Table::default();
    let spell_card = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature gets +2/+2 and gains flying until your next turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let creature = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(creature)]);
    assert_eq!(g.pt(creature), (4, 4));
    assert!(g.has(creature, Keyword::Flying));
    g.until(P1, Step::Upkeep);
    assert_eq!(g.pt(creature), (4, 4));
    assert!(g.has(creature, Keyword::Flying));
    g.until(P0, Step::Upkeep);
    assert_eq!(g.pt(creature), (2, 2));
    assert!(!g.has(creature, Keyword::Flying));
}

#[test]
fn granted_keyword_expires_without_removing_printed_keyword() {
    let mut t = Table::default();
    let spell_card = t.card(
        "{0}",
        "Instant",
        None,
        "Creatures you control gain indestructible until your next turn.",
    );
    let creature_card = t.card("{0}", "Creature", Some((2, 2)), "Flying");
    let mut g = Game::new(t);
    let creature = g.put(creature_card, P0, Zone::Battlefield);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    g.until(P1, Step::Upkeep);
    assert!(g.has(creature, Keyword::Indestructible));
    g.until(P0, Step::Upkeep);
    assert!(!g.has(creature, Keyword::Indestructible));
    assert!(g.has(creature, Keyword::Flying));
}
