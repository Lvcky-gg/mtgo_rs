//! "The next instant or sorcery spell you cast this turn can't be countered." (Overmaster),
//! "… can be cast as though it had flash", "… costs {2} less to cast": used up by the first
//! matching spell its controller casts.

use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::actions::Action;

fn uncounterable(g: &Game, id: mtg_core::ObjectId) -> bool {
    g.engine.state.objects[&id]
        .cast_context
        .as_ref()
        .is_some_and(|c| c.uncounterable)
}

#[test]
fn the_next_matching_spell_cant_be_countered_and_only_it() {
    let mut t = Table::default();
    let overmaster = t.card(
        "{R}",
        "Sorcery",
        None,
        "The next instant or sorcery spell you cast this turn can't be countered.\nDraw a card.",
    );
    let bear = t.bear();
    let shock = t.card("{0}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(3);
    let spell = g.put(overmaster, P0, Zone::Hand);
    let creature = g.put(bear, P0, Zone::Hand);
    let first = g.put(shock, P0, Zone::Hand);
    let second = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    // A creature spell isn't an instant or sorcery: it doesn't use the effect up.
    g.act_holding(Action::Cast { object: creature }, &[]);
    let on_stack = g.stack();
    assert!(!uncounterable(&g, on_stack[0]));
    g.until(P0, Step::PrecombatMain);
    g.act_holding(Action::Cast { object: first }, &[]);
    let on_stack = g.stack();
    assert!(uncounterable(&g, on_stack[0]), "the first instant");
    g.until(P0, Step::PrecombatMain);
    g.act_holding(Action::Cast { object: second }, &[]);
    let on_stack = g.stack();
    assert!(!uncounterable(&g, on_stack[0]), "used up");
}

#[test]
fn the_next_spell_costs_less_once() {
    let mut t = Table::default();
    let discount = t.card(
        "{0}",
        "Instant",
        None,
        "The next spell you cast this turn costs {2} less to cast.",
    );
    let ogre = t.card("{2}{R}", "Creature — Ogre", Some((3, 3)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(discount, P0, Zone::Hand);
    let first = g.put(ogre, P0, Zone::Hand);
    let second = g.put(ogre, P0, Zone::Hand);
    assert!(!g.main().contains(&Action::Cast { object: first }));
    g.cast(spell, &[]);
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object: first }));
    g.cast(first, &[]);
    g.lands(1);
    assert!(
        !g.main().contains(&Action::Cast { object: second }),
        "used up"
    );
}

#[test]
fn the_next_creature_spell_has_flash_this_turn() {
    let mut t = Table::default();
    let quick = t.card(
        "{0}",
        "Instant",
        None,
        "The next creature spell you cast this turn can be cast as though it had flash.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let spell = g.put(quick, P0, Zone::Hand);
    let creature = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    let actions = g.until(P0, Step::End);
    assert!(actions.contains(&Action::Cast { object: creature }));
    g.cast(creature, &[]);
    assert!(g.find(bear).is_some());
}

#[test]
fn an_activated_ability_grants_it_too() {
    let mut t = Table::default();
    let village = t.card(
        "",
        "Land",
        None,
        "{T}: Add {U}.\n{0}, {T}: The next spell you cast this turn can't be countered.",
    );
    let shock = t.card("{0}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    let land = g.put(village, P0, Zone::Battlefield);
    let first = g.put(shock, P0, Zone::Hand);
    g.main();
    g.act(activate(land, 1), &[], &[]);
    g.act_holding(Action::Cast { object: first }, &[]);
    let on_stack = g.stack();
    assert!(uncounterable(&g, on_stack[0]));
}
