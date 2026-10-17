//! "this creature" / "this token" inside a quoted ability: whatever has the ability.

use super::harness::*;
use mtg_core::{Step, Target, Zone};

#[test]
fn heat_shimmer_makes_a_hasty_copy_that_leaves_at_the_end_step() {
    let mut t = Table::default();
    let shimmer = t.card(
        "{2}{R}",
        "Sorcery",
        None,
        "Create a token that's a copy of target creature, except it has haste and \"At the beginning of the end step, exile this token.\"",
    );
    let ogre = t.card("{2}{R}", "Creature — Ogre", Some((3, 3)), "");
    let mut g = Game::new(t);
    g.lands(3);
    let original = g.put(ogre, P1, Zone::Battlefield);
    let spell = g.put(shimmer, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(original)]);
    let copies = |g: &Game| {
        g.engine
            .state
            .battlefield()
            .into_iter()
            .filter(|id| *id != original && g.engine.state.objects[id].card == ogre)
            .count()
    };
    assert_eq!(copies(&g), 1);
    let token = *g
        .engine
        .state
        .battlefield()
        .iter()
        .find(|id| **id != original && g.engine.state.objects[id].card == ogre)
        .unwrap();
    assert!(g.has(token, mtg_core::Keyword::Haste));
    // Through the end step: the token exiles itself; the original stays.
    g.pending = None;
    g.until(P1, Step::Upkeep);
    assert_eq!(copies(&g), 0);
    assert!(g.engine.state.objects.contains_key(&original));
}

#[test]
fn a_granted_tap_ability_deals_damage_from_the_creature_that_has_it() {
    let mut t = Table::default();
    let fusillade = t.card(
        "{3}{R}",
        "Sorcery",
        None,
        "Until end of turn, permanents you control gain \"{T}: This permanent deals 1 damage to any target.\"",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    let shooter = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(fusillade, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    let actions = g.main();
    let shoot = actions
        .iter()
        .find(|a| {
            matches!(a, mtg_engine::actions::Action::ActivateAbility { source, .. } if *source == shooter)
        })
        .expect("the bear can shoot")
        .clone();
    g.act(shoot, &[Target::Player(P1)], &[]);
    assert_eq!(g.life(P1), 19);
    assert!(g.engine.state.objects[&shooter].tapped);
}
