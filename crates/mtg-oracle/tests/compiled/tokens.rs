//! Predefined tokens and token-making keyword actions.
use super::harness::*;
use mtg_core::{CounterKind, Zone};
use mtg_engine::actions::Action;

#[test]
fn amass_makes_an_army_then_grows_it() {
    let mut t = Table::default();
    let call = t.card("{1}{B}", "Sorcery", None, "Amass Zombies 2.");
    let mut g = Game::new(t);
    g.lands(4);
    let c1 = g.put(call, P0, Zone::Hand);
    let c2 = g.put(call, P0, Zone::Hand);
    g.main();
    g.cast(c1, &[]);
    let armies: Vec<_> = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].is_token)
        .collect();
    assert_eq!(armies.len(), 1);
    let army = armies[0];
    assert_eq!(g.pt(army), (2, 2));
    g.cast(c2, &[]);
    let tokens = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].is_token)
        .count();
    assert_eq!(tokens, 1, "the same Army grows");
    assert_eq!(
        g.engine.state.objects[&army].counters[&CounterKind::PlusOnePlusOne],
        4
    );
}

#[test]
fn a_blood_token_rummages() {
    let mut t = Table::default();
    let bite = t.card("{R}", "Sorcery", None, "Create a Blood token.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bite, P0, Zone::Hand);
    g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(b, &[]);
    let blood = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].is_token)
        .expect("a Blood token");
    let hand = g.count(Zone::Hand, P0);
    g.act(activate(blood, 0), &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand, "discarded one, drew one");
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        2,
        "the sorcery and the discard"
    );
    assert!(!g.engine.state.objects.contains_key(&blood), "sacrificed");
}

#[test]
fn adapt_only_without_counters() {
    let mut t = Table::default();
    let shark = t.card("{1}{U}", "Creature — Shark", Some((1, 1)), "{1}: Adapt 2.");
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(shark, P0, Zone::Battlefield);
    g.main();
    g.act(activate(s, 0), &[], &[]);
    assert_eq!(g.pt(s), (3, 3));
    g.act(activate(s, 0), &[], &[]);
    assert_eq!(g.pt(s), (3, 3), "already had counters");
}

#[test]
fn populate_copies_a_creature_token() {
    let mut t = Table::default();
    let make = t.card(
        "{G}",
        "Sorcery",
        None,
        "Create a 3/3 black Zombie creature token.",
    );
    let pop = t.card("{G}", "Sorcery", None, "Populate.");
    let mut g = Game::new(t);
    g.lands(2);
    let m = g.put(make, P0, Zone::Hand);
    let p = g.put(pop, P0, Zone::Hand);
    g.main();
    g.cast(m, &[]);
    g.cast(p, &[]);
    let tokens: Vec<_> = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].is_token)
        .collect();
    assert_eq!(tokens.len(), 2);
    assert_eq!(g.pt(tokens[1]), (3, 3));
}

#[test]
fn copy_the_spell_you_cast() {
    let mut t = Table::default();
    let echo = t.card(
        "{2}{U}{R}",
        "Creature — Wizard",
        Some((2, 2)),
        "Whenever you cast an instant or sorcery spell, copy that spell.",
    );
    let salve = t.card("{W}", "Instant", None, "You gain 2 life.");
    let mut g = Game::new(t);
    g.lands(1);
    g.put(echo, P0, Zone::Battlefield);
    let s = g.put(salve, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!(g.life(P0), 24, "the spell and its copy");
}

#[test]
fn powerstone_mana_pays_for_artifacts_and_abilities_but_not_other_spells() {
    let mut t = Table::default();
    let miner = t.card(
        "{1}",
        "Artifact Creature — Soldier",
        Some((1, 1)),
        "When this creature enters, create a tapped Powerstone token.",
    );
    let relic = t.card("{2}", "Artifact", None, "");
    let sorcery = t.card("{2}", "Sorcery", None, "You gain 1 life.");
    let lamp = t.card("{0}", "Artifact", None, "{2}: You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(1);
    let m = g.put(miner, P0, Zone::Hand);
    let r = g.put(relic, P0, Zone::Hand);
    let s = g.put(sorcery, P0, Zone::Hand);
    let l = g.put(lamp, P0, Zone::Battlefield);
    g.main();
    g.cast(m, &[]);
    // Next turn the land and the Powerstone are untapped: two mana, one restricted.
    g.until(P1, mtg_core::Step::PrecombatMain);
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    assert!(
        actions.contains(&Action::Cast { object: r }),
        "an artifact spell"
    );
    assert!(
        !actions.contains(&Action::Cast { object: s }),
        "not a nonartifact spell"
    );
    assert!(offers(&actions, l), "an ability may use it");
    let life = g.life(P0);
    g.act(activate(l, 0), &[], &[]);
    assert_eq!(g.life(P0), life + 1);
}
