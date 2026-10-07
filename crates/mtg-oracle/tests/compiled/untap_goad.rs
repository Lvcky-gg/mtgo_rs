use super::harness::*;
use mtg_core::{Keyword, Step, Target, Zone};

#[test]
fn stolen_creature_is_untapped_hasty_and_returns_goaded_after_cleanup() {
    let mut t = Table::default();
    let card = t.card("{0}", "Sorcery", None,
        "Until end of turn, gain control of target creature and it gains haste. Untap and goad that creature.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let creature = g.put(bear, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&creature).unwrap().tapped = true;
    let spell = g.put(card, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(creature)]);
    assert!(!g.engine.state.objects[&creature].tapped);
    assert_eq!(
        mtg_engine::layers::controller(&g.engine.state, creature),
        Some(P0)
    );
    assert!(g.has(creature, Keyword::Haste));
    g.until(P1, Step::PrecombatMain);
    assert_eq!(
        mtg_engine::layers::controller(&g.engine.state, creature),
        Some(P1)
    );
    assert!(!g.has(creature, Keyword::Haste));
    assert_eq!(
        mtg_engine::combat::must_attack(&g.engine.state, &g.table, P1),
        vec![creature]
    );
    g.until(P0, Step::Upkeep);
    assert!(mtg_engine::combat::must_attack(&g.engine.state, &g.table, P1).is_empty());
}

#[test]
fn shared_target_untaps_and_goads_only_selected_creature() {
    let mut t = Table::default();
    let card = t.card("{0}", "Instant", None, "Untap and goad target creature.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let chosen = g.put(bear, P1, Zone::Battlefield);
    let other = g.put(bear, P1, Zone::Battlefield);
    for id in [chosen, other] {
        g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
    }
    let spell = g.put(card, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(chosen)]);
    assert!(!g.engine.state.objects[&chosen].tapped);
    assert!(g.engine.state.objects[&other].tapped);
    assert_eq!(
        mtg_engine::combat::must_attack(&g.engine.state, &g.table, P1),
        vec![chosen]
    );
}
