use super::harness::*;
use mtg_core::{CounterKind, Event, Step, Target, Zone};

#[test]
fn phasing_alternates_on_your_untaps_without_changing_identity_or_counters() {
    let mut t = Table::default();
    let raider = t.card(
        "{0}",
        "Creature",
        Some((2, 2)),
        "Phasing\nThis creature enters with a +1/+1 counter on it.",
    );
    let mut g = Game::new(t);
    let spell = g.put(raider, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    let raider = g.find(raider).unwrap();
    mtg_engine::apply::apply(
        &mut g.engine.state,
        mtg_core::Cause::PlayerAction(P0),
        Event::TapChanged {
            object: raider,
            tapped: true,
        },
        &mut g.engine.log,
    );
    let after_enter = g.engine.log.len();
    g.until(P1, Step::PrecombatMain);
    assert!(!g.engine.state.objects[&raider].phased_out);
    g.until(P0, Step::PrecombatMain);
    assert!(g.engine.state.objects[&raider].phased_out);
    assert!(g.engine.state.objects[&raider].tapped);
    assert!(!g.engine.state.battlefield().contains(&raider));
    g.until(P1, Step::PrecombatMain);
    assert!(g.engine.state.objects[&raider].phased_out);
    g.until(P0, Step::PrecombatMain);
    assert!(!g.engine.state.objects[&raider].phased_out);
    assert!(!g.engine.state.objects[&raider].tapped);
    assert_eq!(
        g.engine.state.objects[&raider].counters[&CounterKind::PlusOnePlusOne],
        1
    );
    assert_eq!(g.pt(raider), (3, 3));
    assert!(
        !g.engine.log[after_enter..]
            .iter()
            .any(|e| matches!(e.event, Event::ZoneChange { object, .. } if object == raider))
    );
}

#[test]
fn indirectly_phased_aura_waits_for_its_host_even_with_another_controller() {
    let mut t = Table::default();
    let raider = t.card("{0}", "Creature", Some((2, 2)), "Phasing");
    let aura = t.card(
        "{0}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature gets +1/+1.",
    );
    let mut g = Game::new(t);
    let raider = g.put(raider, P0, Zone::Battlefield);
    let aura = g.put(aura, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&aura).unwrap().attached_to = Some(raider);
    g.main();
    assert!(g.engine.state.objects[&raider].phased_out);
    assert!(g.engine.state.objects[&aura].phased_out);
    assert_eq!(g.engine.state.objects[&aura].phased_with, Some(raider));
    g.until(P1, Step::PrecombatMain);
    assert!(g.engine.state.objects[&aura].phased_out);
    g.until(P0, Step::PrecombatMain);
    assert!(!g.engine.state.objects[&raider].phased_out);
    assert!(!g.engine.state.objects[&aura].phased_out);
    assert_eq!(g.engine.state.objects[&aura].attached_to, Some(raider));
    assert_eq!(g.pt(raider), (3, 3));
}

#[test]
fn phasing_back_in_uses_the_controller_when_it_phased_out_after_temporary_control_ends() {
    let mut t = Table::default();
    let bear = t.bear();
    let steal = t.card(
        "{0}",
        "Instant",
        None,
        "Gain control of target creature until end of turn.",
    );
    let phase = t.card("{0}", "Instant", None, "Target creature phases out.");
    let mut g = Game::new(t);
    let creature = g.put(bear, P1, Zone::Battlefield);
    let steal = g.put(steal, P0, Zone::Hand);
    let phase = g.put(phase, P0, Zone::Hand);
    g.main();
    g.cast(steal, &[Target::Object(creature)]);
    g.cast(phase, &[Target::Object(creature)]);
    assert_eq!(g.engine.state.objects[&creature].phased_under, Some(P0));
    g.until(P1, Step::PrecombatMain);
    assert_eq!(
        mtg_engine::layers::controller(&g.engine.state, creature),
        Some(P1)
    );
    assert!(g.engine.state.objects[&creature].phased_out);
    g.until(P0, Step::PrecombatMain);
    assert!(!g.engine.state.objects[&creature].phased_out);
    assert_eq!(
        mtg_engine::layers::controller(&g.engine.state, creature),
        Some(P1)
    );
}

#[test]
fn phased_out_permanents_do_not_trigger_for_other_objects() {
    let mut t = Table::default();
    let observer = t.card(
        "{0}",
        "Enchantment",
        None,
        "Phasing\nWhenever a creature enters, you gain 1 life.",
    );
    let bear = t.card("{0}", "Creature", Some((2, 2)), "");
    let mut g = Game::new(t);
    let observer = g.put(observer, P0, Zone::Battlefield);
    let first = g.put(bear, P0, Zone::Hand);
    let second = g.put(bear, P0, Zone::Hand);
    g.main();
    assert!(g.engine.state.objects[&observer].phased_out);
    g.cast(first, &[]);
    assert_eq!(g.life(P0), 20);
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    assert!(!g.engine.state.objects[&observer].phased_out);
    g.cast(second, &[]);
    assert_eq!(g.life(P0), 21);
}

#[test]
fn taniwhas_upkeep_trigger_works_only_on_the_turns_it_is_phased_in() {
    let mut t = Table::default();
    let taniwha = t.card(
        "{0}",
        "Creature",
        Some((7, 7)),
        "Phasing\nAt the beginning of your upkeep, all lands you control phase out.",
    );
    let land = t.card("", "Land", None, "{T}: Add {C}.");
    let mut g = Game::new(t);
    let taniwha = g.put(taniwha, P0, Zone::Battlefield);
    let land = g.put(land, P0, Zone::Battlefield);
    g.main();
    assert!(g.engine.state.objects[&taniwha].phased_out);
    assert!(!g.engine.state.objects[&land].phased_out);
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    assert!(!g.engine.state.objects[&taniwha].phased_out);
    assert!(g.engine.state.objects[&land].phased_out);
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    assert!(g.engine.state.objects[&taniwha].phased_out);
    assert!(!g.engine.state.objects[&land].phased_out);
}
