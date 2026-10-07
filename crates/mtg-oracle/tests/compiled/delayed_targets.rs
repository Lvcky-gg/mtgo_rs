use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_engine::actions::Action;

#[test]
fn delayed_destruction_captures_the_object_and_does_not_retarget() {
    for protect in [false, true] {
        let mut t = Table::default();
        let spell_card = t.card(
            "{0}",
            "Instant",
            None,
            "Destroy target creature at the beginning of the next end step.",
        );
        let protect_card = t.card(
            "{0}",
            "Instant",
            None,
            "Target creature gains hexproof until end of turn.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        let victim = g.put(bear, P1, Zone::Battlefield);
        let other = g.put(bear, P1, Zone::Battlefield);
        let spell = g.put(spell_card, P0, Zone::Hand);
        let protection = g.put(protect_card, P0, Zone::Hand);
        g.main();
        g.cast(spell, &[Target::Object(victim)]);
        assert!(g.engine.state.objects.contains_key(&victim));
        assert_eq!(g.engine.state.delayed.len(), 1);
        if protect {
            g.cast(protection, &[Target::Object(victim)]);
        }
        g.until(P0, Step::PostcombatMain);
        g.until(P0, Step::End);
        assert!(!g.engine.state.objects.contains_key(&victim));
        assert!(g.engine.state.objects.contains_key(&other));
    }
}

#[test]
fn leaving_and_returning_makes_a_new_object_that_the_delayed_effect_does_not_destroy() {
    let mut t = Table::default();
    let spell_card = t.card(
        "{0}",
        "Instant",
        None,
        "Destroy target creature at the beginning of the next end step.",
    );
    let flicker_card = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target creature, then return it to the battlefield under its owner's control.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let victim = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(spell_card, P0, Zone::Hand);
    let flicker = g.put(flicker_card, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(victim)]);
    g.cast(flicker, &[Target::Object(victim)]);
    let returned = g.find(bear).unwrap();
    assert_ne!(returned, victim);
    g.until(P0, Step::PostcombatMain);
    g.until(P0, Step::End);
    assert!(g.engine.state.objects.contains_key(&returned));
}

#[test]
fn illegal_initial_target_creates_no_delayed_effect() {
    let mut t = Table::default();
    let delayed_card = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand at end of combat.",
    );
    let destroy_card = t.card("{0}", "Instant", None, "Destroy target creature.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let victim = g.put(bear, P1, Zone::Battlefield);
    let delayed = g.put(delayed_card, P0, Zone::Hand);
    let destroy = g.put(destroy_card, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: delayed }, &[Target::Object(victim)]);
    g.cast(destroy, &[Target::Object(victim)]);
    assert!(g.engine.state.delayed.is_empty());
}
