//! "Exile the top card of your library. You may play that card this turn."
use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};
use mtg_engine::actions::Action;

fn on_top(g: &mut Game, card: mtg_core::CardId) -> mtg_core::ObjectId {
    let id = g.put(card, P0, Zone::Library);
    let lib = ZoneRef::of(Zone::Library, P0);
    let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
    order.retain(|o| *o != id);
    order.insert(0, id);
    id
}

fn exiled(g: &Game, card: mtg_core::CardId) -> Option<mtg_core::ObjectId> {
    g.engine
        .state
        .objects_in(ZoneRef::shared(Zone::Exile))
        .into_iter()
        .find(|id| g.engine.state.objects[id].card == card)
}

#[test]
fn an_exiled_spell_can_be_cast_this_turn_only() {
    let mut t = Table::default();
    let impulse = t.card(
        "{R}",
        "Sorcery",
        None,
        "Exile the top card of your library. You may play that card this turn.",
    );
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(2);
    let i = g.put(impulse, P0, Zone::Hand);
    g.main();
    on_top(&mut g, bolt);
    g.cast(i, &[]);
    let b = exiled(&g, bolt).expect("exiled");
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    assert!(actions.contains(&Action::Cast { object: b }));
    g.cast(b, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 17);
}

#[test]
fn the_permission_ends_with_the_turn() {
    let mut t = Table::default();
    let impulse = t.card(
        "{R}",
        "Sorcery",
        None,
        "Exile the top card of your library. You may play that card this turn.",
    );
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(1);
    let i = g.put(impulse, P0, Zone::Hand);
    g.main();
    on_top(&mut g, bolt);
    g.cast(i, &[]);
    let b = exiled(&g, bolt).unwrap();
    g.until(P0, mtg_core::Step::End);
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    assert!(
        !actions.contains(&Action::Cast { object: b }),
        "next turn: too late"
    );
}

#[test]
fn an_exiled_land_can_be_played() {
    let mut t = Table::default();
    let impulse = t.card(
        "{R}",
        "Sorcery",
        None,
        "Exile the top card of your library. You may play that card this turn.",
    );
    let mountain = t.mountain();
    let mut g = Game::new(t);
    g.lands(1);
    let i = g.put(impulse, P0, Zone::Hand);
    g.main();
    on_top(&mut g, mountain);
    g.cast(i, &[]);
    let m = exiled(&g, mountain).unwrap();
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    assert!(actions.contains(&Action::PlayLand { object: m }));
}

#[test]
fn creature_spells_from_the_top_of_the_library() {
    let mut t = Table::default();
    let seer = t.card(
        "{2}{G}",
        "Creature — Elf",
        Some((1, 1)),
        "You may look at the top card of your library any time.\nYou may cast creature spells \
         from the top of your library.",
    );
    let bear = t.bear();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(2);
    g.put(seer, P0, Zone::Battlefield);
    // The draw step takes the bolt; the bear is left on top.
    let b = on_top(&mut g, bear);
    on_top(&mut g, bolt);
    let actions = g.main();
    assert_eq!(
        g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[0],
        b
    );
    let mine = mtg_engine::view::project(&g.engine.state, P0);
    let theirs = mtg_engine::view::project(&g.engine.state, P1);
    assert!(
        mine.visible.get(&b).is_some_and(|v| v.card == Some(bear)),
        "P0 sees it"
    );
    assert!(!theirs.visible.contains_key(&b), "P1 does not");
    assert!(actions.contains(&Action::Cast { object: b }));
    g.cast(b, &[]);
    assert!(g.find(bear).is_some());
}
