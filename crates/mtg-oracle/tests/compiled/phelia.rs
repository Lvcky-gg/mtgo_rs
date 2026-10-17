use super::harness::*;
use mtg_core::{CounterKind, Step, Target, Zone};

const TEXT: &str = "Flash\nWhenever Phelia attacks, exile up to one other target nonland permanent. At the beginning of the next end step, return that card to the battlefield under its owner's control. If it entered under your control, put a +1/+1 counter on Phelia.";

#[test]
fn return_and_counter_wait_for_end_step_and_use_the_returned_cards_controller() {
    for owner in [P0, P1] {
        let mut t = Table::default();
        let card = t.named_card(
            "Phelia, Exuberant Shepherd",
            "{1}{W}",
            "Legendary Creature — Dog",
            Some((2, 2)),
            TEXT,
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        let phelia = g.put(card, P0, Zone::Battlefield);
        let victim = g.put(bear, owner, Zone::Battlefield);
        // A stolen card goes back to its owner, regardless of its old controller.
        g.engine.state.objects.get_mut(&victim).unwrap().controller = P0;
        g.main();
        g.combat(&[phelia], &[], &[Target::Object(victim)], &[]);
        assert!(g.find(bear).is_none());
        assert_eq!(g.engine.state.battlefield().len(), 1);
        assert_eq!(g.pt(phelia), (2, 2));
        assert_eq!(g.engine.state.delayed.len(), 1);
        g.until(P0, Step::End);
        let returned = g.find(bear).unwrap();
        assert_ne!(returned, victim);
        assert_eq!(g.engine.state.objects[&returned].controller, owner);
        assert_eq!(
            g.engine.state.objects[&returned].zone.zone,
            Zone::Battlefield
        );
        assert_eq!(g.pt(phelia), if owner == P0 { (3, 3) } else { (2, 2) });
    }
}

#[test]
fn attacking_without_a_legal_target_does_not_add_a_counter() {
    let mut t = Table::default();
    let card = t.named_card(
        "Phelia, Exuberant Shepherd",
        "{1}{W}",
        "Legendary Creature — Dog",
        Some((2, 2)),
        TEXT,
    );
    let mut g = Game::new(t);
    let phelia = g.put(card, P0, Zone::Battlefield);
    g.main();
    g.combat(&[phelia], &[], &[], &[]);
    g.until(P0, Step::End);
    assert_eq!(
        g.engine.state.objects[&phelia]
            .counters
            .get(&CounterKind::PlusOnePlusOne)
            .copied()
            .unwrap_or(0),
        0
    );
}

#[test]
fn a_card_removed_from_exile_is_not_returned() {
    let mut t = Table::default();
    let card = t.named_card(
        "Phelia, Exuberant Shepherd",
        "{1}{W}",
        "Legendary Creature — Dog",
        Some((2, 2)),
        TEXT,
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let phelia = g.put(card, P0, Zone::Battlefield);
    let victim = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.combat(&[phelia], &[], &[Target::Object(victim)], &[]);
    let exiled = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == bear)
        .unwrap()
        .id;
    let hand = g.engine.state.new_object_id();
    mtg_engine::apply::apply(
        &mut g.engine.state,
        mtg_core::Cause::PlayerAction(P0),
        mtg_core::Event::ZoneChange {
            object: exiled,
            new_object: hand,
            from: mtg_core::ZoneRef::shared(Zone::Exile),
            to: mtg_core::ZoneRef::of(Zone::Hand, P0),
            index: None,
        },
        &mut g.engine.log,
    );
    g.until(P0, Step::End);
    assert_eq!(g.engine.state.objects[&hand].zone.zone, Zone::Hand);
    assert_eq!(g.pt(phelia), (2, 2));
}

#[test]
fn the_delayed_return_survives_phelia_leaving_without_countering_its_new_incarnation() {
    let mut t = Table::default();
    let card = t.named_card(
        "Phelia, Exuberant Shepherd",
        "{1}{W}",
        "Legendary Creature — Dog",
        Some((2, 2)),
        TEXT,
    );
    let bear = t.bear();
    let blink = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target creature, then return it to the battlefield under its owner's control.",
    );
    let mut g = Game::new(t);
    let phelia = g.put(card, P0, Zone::Battlefield);
    let victim = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(blink, P0, Zone::Hand);
    g.main();
    g.combat(&[phelia], &[], &[Target::Object(victim)], &[]);
    g.cast(spell, &[Target::Object(phelia)]);
    let new_phelia = g.find(card).unwrap();
    assert_ne!(phelia, new_phelia);
    g.until(P0, Step::End);
    let returned = g.find(bear).unwrap();
    assert_eq!(
        g.engine.state.objects[&returned].zone.zone,
        Zone::Battlefield
    );
    assert_eq!(g.pt(new_phelia), (2, 2));
}
