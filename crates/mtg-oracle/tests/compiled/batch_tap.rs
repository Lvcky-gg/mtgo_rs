use super::harness::*;
use mtg_core::{Zone, ZoneRef};

#[test]
fn deeproot_pilgrimage_triggers_once_per_simultaneous_tap_and_ignores_tokens() {
    let mut t = Table::default();
    let pilgrimage = t.card("{1}{U}", "Enchantment", None,
        "Whenever one or more nontoken Merfolk you control become tapped, create a 1/1 blue Merfolk creature token with hexproof.");
    let merfolk = t.card("{U}", "Creature — Merfolk", Some((2, 2)), "");
    let tap = t.card("{U}", "Instant", None, "Tap all creatures.");
    let mut g = Game::new(t);
    g.lands(3);
    g.put(pilgrimage, P0, Zone::Battlefield);
    let a = g.put(merfolk, P0, Zone::Battlefield);
    let b = g.put(merfolk, P0, Zone::Battlefield);
    g.put(merfolk, P1, Zone::Battlefield);
    let spells: Vec<_> = (0..3).map(|_| g.put(tap, P0, Zone::Hand)).collect();
    g.main();
    let count = |g: &Game| {
        g.engine
            .state
            .objects
            .values()
            .filter(|o| {
                o.zone == ZoneRef::shared(Zone::Battlefield) && o.is_token && o.controller == P0
            })
            .count()
    };
    g.cast(spells[0], &[]);
    assert_eq!(
        count(&g),
        1,
        "two simultaneous Merfolk taps give one trigger"
    );
    g.cast(spells[1], &[]);
    assert_eq!(
        count(&g),
        1,
        "only the new token taps, which does not trigger"
    );
    g.engine.state.objects.get_mut(&a).unwrap().tapped = false;
    g.engine.state.objects.get_mut(&b).unwrap().tapped = false;
    g.cast(spells[2], &[]);
    assert_eq!(count(&g), 2, "a later tap event can trigger again");
}

#[test]
fn simultaneous_untap_step_changes_trigger_once() {
    let mut t = Table::default();
    let watcher = t.card(
        "{1}",
        "Enchantment",
        None,
        "Whenever one or more creatures you control become untapped, you gain 2 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(watcher, P0, Zone::Battlefield);
    for _ in 0..2 {
        let creature = g.put(bear, P0, Zone::Battlefield);
        g.engine.state.objects.get_mut(&creature).unwrap().tapped = true;
    }
    g.main();
    assert_eq!(g.engine.state.players[&P0].life, 22);
}
