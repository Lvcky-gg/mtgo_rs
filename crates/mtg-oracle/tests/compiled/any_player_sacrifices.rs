//! "Whenever a player sacrifices a permanent" (Mayhem Devil): yours and theirs alike.

use super::harness::*;
use mtg_core::{Target, Zone};

const DEVIL: &str =
    "Whenever a player sacrifices a permanent, this creature deals 1 damage to any target.";

#[test]
fn mayhem_devil_triggers_when_an_opponent_sacrifices() {
    let mut t = Table::default();
    let devil = t.card("{1}{B}{R}", "Creature — Devil", Some((3, 3)), DEVIL);
    let edict = t.card(
        "{0}",
        "Sorcery",
        None,
        "Each opponent sacrifices a creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(devil, P0, Zone::Battlefield);
    let victim = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(edict, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Player(P1)]);
    assert!(!g.engine.state.objects.contains_key(&victim));
    assert_eq!(g.life(P1), 19);
}

#[test]
fn mayhem_devil_triggers_when_you_sacrifice() {
    let mut t = Table::default();
    let devil = t.card("{1}{B}{R}", "Creature — Devil", Some((3, 3)), DEVIL);
    let fodder = t.card(
        "{0}",
        "Artifact",
        None,
        "Sacrifice this artifact: You gain 1 life.",
    );
    let mut g = Game::new(t);
    g.put(devil, P0, Zone::Battlefield);
    let source = g.put(fodder, P0, Zone::Battlefield);
    g.main();
    g.act(activate(source, 0), &[Target::Player(P1)], &[]);
    assert_eq!(g.life(P1), 19);
    assert_eq!(g.life(P0), 21);
}
