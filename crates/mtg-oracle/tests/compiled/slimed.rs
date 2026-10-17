use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_engine::actions::Action;

const SLIMED: &str = "Counter target activated or triggered ability. If a permanent's ability is countered this way, activated abilities of that permanent can't be activated this turn.";
const SOURCE: &str = "{0}: You gain 1 life.\n{T}: Add {C}.";

#[test]
fn countering_an_activation_disables_all_activations_of_only_its_source_until_cleanup() {
    let mut t = Table::default();
    let source = t.card("{0}", "Artifact", None, SOURCE);
    let slimed = t.card("{0}", "Instant", None, SLIMED);
    let mut g = Game::new(t);
    let a = g.put(source, P0, Zone::Battlefield);
    let b = g.put(source, P0, Zone::Battlefield);
    let spell = g.put(slimed, P0, Zone::Hand);
    g.main();
    g.act_holding(activate(a, 0), &[]);
    let ability = g.stack()[0];
    g.cast(spell, &[Target::Object(ability)]);
    assert_eq!(g.life(P0), 20);
    let actions = g.main();
    assert!(!actions.contains(&activate(a, 0)));
    assert!(!mana_offered(&g, a), "mana abilities are disabled too");
    assert!(actions.contains(&activate(b, 0)));
    assert!(mana_offered(&g, b));
    let actions = g.until(P0, Step::PrecombatMain);
    // Already in this main phase: advance through the next player's turn first.
    assert!(!actions.contains(&activate(a, 0)));
    g.until(P1, Step::Upkeep);
    let actions = g.main();
    assert!(actions.contains(&activate(a, 0)));
    assert!(mana_offered(&g, a));
}

#[test]
fn an_uncounterable_ability_resolves_and_does_not_disable_its_source() {
    let mut t = Table::default();
    let source = t.card("{0}", "Artifact", None, SOURCE);
    let slimed = t.card("{0}", "Instant", None, SLIMED);
    let mut g = Game::new(t);
    let source = g.put(source, P0, Zone::Battlefield);
    let spell = g.put(slimed, P0, Zone::Hand);
    g.main();
    g.act_holding(activate(source, 0), &[]);
    let ability = g.stack()[0];
    g.engine
        .state
        .objects
        .get_mut(&ability)
        .unwrap()
        .cast_context
        .as_mut()
        .unwrap()
        .uncounterable = true;
    g.cast(spell, &[Target::Object(ability)]);
    assert_eq!(g.life(P0), 21);
    assert!(g.engine.state.continuous.is_empty());
    assert!(g.main().contains(&activate(source, 0)));
}

#[test]
fn a_returned_source_has_a_new_identity_and_can_activate() {
    let mut t = Table::default();
    let source = t.card("{0}", "Artifact", None, SOURCE);
    let slimed = t.card("{0}", "Instant", None, SLIMED);
    let blink = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target artifact, then return it to the battlefield under its owner's control.",
    );
    let mut g = Game::new(t);
    let original = g.put(source, P0, Zone::Battlefield);
    let spell = g.put(slimed, P0, Zone::Hand);
    let blink = g.put(blink, P0, Zone::Hand);
    g.main();
    g.act_holding(activate(original, 0), &[]);
    g.cast(spell, &[Target::Object(g.stack()[0])]);
    g.cast(blink, &[Target::Object(original)]);
    let returned = g.find(source).unwrap();
    assert_ne!(returned, original);
    assert!(g.main().contains(&activate(returned, 0)));
}

#[test]
fn countering_a_trigger_also_disables_the_sources_activated_abilities() {
    let mut t = Table::default();
    let source = t.card(
        "{0}",
        "Creature — Bear",
        Some((2, 2)),
        "When this creature enters, you gain 3 life.\n{0}: You gain 1 life.",
    );
    let slimed = t.card("{0}", "Instant", None, SLIMED);
    let mut g = Game::new(t);
    let source = g.put(source, P0, Zone::Hand);
    let spell = g.put(slimed, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: source }, &[]);
    g.drive(
        |_, _| None,
        |g| {
            g.stack().iter().any(|id| {
                g.engine.state.objects[id]
                    .cast_context
                    .as_ref()
                    .is_some_and(|c| c.ability.is_some())
            })
        },
    );
    let source = g.engine.state.battlefield()[0];
    let ability = g.stack()[0];
    g.cast(spell, &[Target::Object(ability)]);
    assert_eq!(g.life(P0), 20);
    assert!(!g.main().contains(&activate(source, 1)));
}

fn mana_offered(g: &Game, source: mtg_core::ObjectId) -> bool {
    match &g.pending.as_ref().unwrap().kind {
        mtg_engine::choice::ChoiceKind::Priority { legal } => legal
            .mana_abilities
            .iter()
            .any(|a| matches!(a, Action::ActivateManaAbility { source: s, .. } if *s == source)),
        _ => false,
    }
}
