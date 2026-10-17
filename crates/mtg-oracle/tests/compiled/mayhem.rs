use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::{actions::Action, choice::Answer};

fn grave(g: &Game, card: mtg_core::CardId) -> mtg_core::ObjectId {
    g.engine
        .state
        .objects
        .values()
        .find(|o| o.card == card && o.zone.zone == Zone::Graveyard)
        .unwrap()
        .id
}

#[test]
fn mayhem_casts_the_discarded_card_for_its_cost_and_keeps_other_graveyard_copies_unavailable() {
    let mut t = Table::default();
    let swarm = t.named_card(
        "Swarm, Being of Bees",
        "{2}{B}",
        "Legendary Creature — Insect Villain",
        Some((2, 2)),
        "Flash\nFlying\nMayhem {B}",
    );
    let discard = t.card("{0}", "Sorcery", None, "Discard a card.");
    let mut g = Game::new(t);
    g.lands(1);
    let old = g.put(swarm, P0, Zone::Graveyard);
    let hand = g.put(swarm, P0, Zone::Hand);
    let discard = g.put(discard, P0, Zone::Hand);
    g.main();
    assert!(!g.main().contains(&Action::Cast { object: old }));
    g.act(
        Action::Cast { object: discard },
        &[],
        &[Answer::Objects(vec![hand])],
    );
    let discarded = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == swarm && o.zone.zone == Zone::Graveyard && o.id != old)
        .unwrap()
        .id;
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object: discarded }));
    assert!(!actions.contains(&Action::Cast { object: old }));
    g.cast(discarded, &[]);
    assert_eq!(
        g.engine.state.objects[&g.find(swarm).unwrap()].zone.zone,
        Zone::Battlefield
    );
}

#[test]
fn a_card_put_into_the_graveyard_without_being_discarded_has_no_mayhem_permission() {
    let mut t = Table::default();
    let body = t.card(
        "{4}",
        "Legendary Creature — Insect",
        Some((2, 2)),
        "Mayhem {0}",
    );
    let mill = t.card("{0}", "Sorcery", None, "Mill a card.");
    let mut g = Game::new(t);
    let mill = g.put(mill, P0, Zone::Hand);
    // Put the fixture on top after the draw step, using the actual library order.
    g.main();
    let library = g.put(body, P0, Zone::Library);
    let zone = mtg_core::ZoneRef::of(Zone::Library, P0);
    let order = g.engine.state.zone_order.get_mut(&zone).unwrap();
    order.retain(|id| *id != library);
    order.insert(0, library);
    g.cast(mill, &[]);
    let milled = grave(&g, body);
    assert!(!g.main().contains(&Action::Cast { object: milled }));
}

#[test]
fn mayhem_is_lost_on_resolution_and_is_not_flashback_exile() {
    let mut t = Table::default();
    let spell = t.card("{4}", "Sorcery", None, "You gain 3 life.\nMayhem {0}");
    let discard = t.card("{0}", "Instant", None, "Discard a card.");
    let mut g = Game::new(t);
    let hand = g.put(spell, P0, Zone::Hand);
    let discard = g.put(discard, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: discard },
        &[],
        &[Answer::Objects(vec![hand])],
    );
    let discarded = grave(&g, spell);
    g.cast(discarded, &[]);
    assert_eq!(g.life(P0), 23);
    let returned = grave(&g, spell);
    assert_ne!(returned, discarded);
    assert!(!g.main().contains(&Action::Cast { object: returned }));
}

#[test]
fn mayhem_expires_next_turn_and_does_not_override_creature_timing() {
    let mut t = Table::default();
    let body = t.card(
        "{4}",
        "Legendary Creature — Human",
        Some((2, 2)),
        "Mayhem {0}",
    );
    let discard = t.card("{0}", "Instant", None, "Discard a card.");
    let mut g = Game::new(t);
    let hand = g.put(body, P0, Zone::Hand);
    let discard = g.put(discard, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: discard },
        &[],
        &[Answer::Objects(vec![hand])],
    );
    let discarded = grave(&g, body);
    assert!(g.main().contains(&Action::Cast { object: discarded }));
    g.until(P0, Step::BeginCombat);
    assert!(!actions(&g).contains(&Action::Cast { object: discarded }));
    g.until(P1, Step::Upkeep);
    let actions = g.main();
    assert!(!actions.contains(&Action::Cast { object: discarded }));
}

#[test]
fn discarding_as_an_activation_cost_grants_mayhem() {
    let mut t = Table::default();
    let body = t.card(
        "{4}",
        "Legendary Creature — Human",
        Some((2, 2)),
        "Mayhem {0}",
    );
    let looter = t.card(
        "{0}",
        "Artifact",
        None,
        "{T}, Discard a card: You gain 1 life.",
    );
    let mut g = Game::new(t);
    let hand = g.put(body, P0, Zone::Hand);
    let looter = g.put(looter, P0, Zone::Battlefield);
    g.main();
    g.act(activate(looter, 0), &[], &[Answer::Objects(vec![hand])]);
    let discarded = grave(&g, body);
    assert!(g.main().contains(&Action::Cast { object: discarded }));
}

fn actions(g: &Game) -> Vec<Action> {
    match &g.pending.as_ref().unwrap().kind {
        mtg_engine::choice::ChoiceKind::Priority { legal } => legal.actions.clone(),
        _ => Vec::new(),
    }
}

#[test]
fn flash_allows_mayhem_during_an_opponents_turn() {
    let mut t = Table::default();
    let body = t.card(
        "{4}",
        "Creature — Insect",
        Some((2, 2)),
        "Flash\nMayhem {0}",
    );
    let discard = t.card("{0}", "Instant", None, "Discard a card.");
    let mut g = Game::new(t);
    let hand = g.put(body, P0, Zone::Hand);
    let discard = g.put(discard, P0, Zone::Hand);
    g.main();
    g.until(P1, Step::Upkeep);
    let prompt = g.pending.take().unwrap();
    g.engine.answer(&g.table, prompt.id, Answer::Pass).unwrap();
    g.drive(|_, _| None, |_| true);
    g.act(
        Action::Cast { object: discard },
        &[],
        &[Answer::Objects(vec![hand])],
    );
    let discarded = grave(&g, body);
    assert!(actions(&g).contains(&Action::Cast { object: discarded }));
    g.cast(discarded, &[]);
    assert_eq!(
        g.engine.state.objects[&g.find(body).unwrap()].zone.zone,
        Zone::Battlefield
    );
}

#[test]
fn mayhem_still_requires_the_spells_additional_cost() {
    let mut t = Table::default();
    let spell = t.card("{4}", "Sorcery", None, "As an additional cost to cast this spell, sacrifice a creature.\nYou gain 3 life.\nMayhem {0}");
    let body_card = t.card("{0}", "Creature — Insect", Some((1, 1)), "");
    let discard = t.card("{0}", "Instant", None, "Discard a card.");
    let mut g = Game::new(t);
    let hand = g.put(spell, P0, Zone::Hand);
    let body = g.put(body_card, P0, Zone::Battlefield);
    let discard = g.put(discard, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: discard },
        &[],
        &[Answer::Objects(vec![hand])],
    );
    let discarded = grave(&g, spell);
    g.act(
        Action::Cast { object: discarded },
        &[],
        &[Answer::Objects(vec![body])],
    );
    assert_eq!(g.life(P0), 23);
    assert_ne!(grave(&g, body_card), body);
}
