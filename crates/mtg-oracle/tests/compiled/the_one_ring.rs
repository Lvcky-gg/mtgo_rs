use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::choice::ChoiceKind;

const RING: &str = "Indestructible\nWhen ~ enters, if you cast it, you gain protection from everything until your next turn.\nAt the beginning of your upkeep, you lose 1 life for each burden counter on ~.\n{T}: Put a burden counter on ~, then draw a card for each burden counter on ~.";

#[test]
fn the_one_ring_draws_after_adding_the_counter_and_loses_life_at_upkeep() {
    let mut t = Table::default();
    let ring = t.card("{4}", "Legendary Artifact", None, RING);
    let mut g = Game::new(t);
    g.lands(4);
    let spell = g.put(ring, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    let ring = g.find(ring).unwrap();
    let hand = g.count(Zone::Hand, P0);
    g.act(activate(ring, 3), &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    assert_eq!(g.life(P0), 19, "protection does not stop losing life");
    let hand = g.count(Zone::Hand, P0);
    g.act(activate(ring, 3), &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand + 2);
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::Upkeep);
    assert_eq!(g.life(P0), 17);
}

#[test]
fn the_one_ring_protection_requires_casting_and_expires_on_your_next_turn() {
    for cast in [false, true] {
        let mut t = Table::default();
        let ring = t.card("{4}", "Legendary Artifact", None, RING);
        let reanimate = t.card(
            "{0}",
            "Sorcery",
            None,
            "Return target artifact card from your graveyard to the battlefield.",
        );
        let bolt = t.card("{0}", "Instant", None, "~ deals 1 damage to target player.");
        let mut g = Game::new(t);
        g.lands(4);
        let ring = g.put(ring, P0, if cast { Zone::Hand } else { Zone::Graveyard });
        let reanimate = g.put(reanimate, P0, Zone::Hand);
        let bolt = g.put(bolt, P1, Zone::Hand);
        g.main();
        if cast {
            g.cast(ring, &[]);
        } else {
            g.cast(reanimate, &[mtg_core::Target::Object(ring)]);
        }
        g.until(P1, Step::PrecombatMain);
        // A spell whose only legal target would be the protected player still has P1
        // available, so inspect the target choices for this spell directly.
        let spec = mtg_ir::selector::TargetSpec {
            zone: Zone::Battlefield,
            filter: mtg_ir::ObjectFilter::Any,
            allows_players: true,
            players: Some(mtg_ir::Selector::EachPlayer),
            mode: None,
            count: mtg_ir::Value::ONE,
            up_to: false,
            distinct_from_other_targets: false,
        };
        let legal =
            mtg_engine::targeting::legal_targets(&g.engine.state, &g.table, &spec, bolt, P1, &[]);
        assert_eq!(legal.contains(&mtg_core::Target::Player(P0)), !cast);
        g.until(P0, Step::PrecombatMain);
        let legal =
            mtg_engine::targeting::legal_targets(&g.engine.state, &g.table, &spec, bolt, P1, &[]);
        assert!(legal.contains(&mtg_core::Target::Player(P0)));
        assert!(matches!(
            g.pending.as_ref().unwrap().kind,
            ChoiceKind::Priority { .. }
        ));
    }
}

#[test]
fn returning_the_ring_in_response_draws_using_its_last_known_burden_count() {
    let mut t = Table::default();
    let ring = t.card("{4}", "Legendary Artifact", None, RING);
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target artifact to its owner's hand.",
    );
    let mut g = Game::new(t);
    let ring = g.put(ring, P0, Zone::Battlefield);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.act(activate(ring, 3), &[], &[]);
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    let hand = g.count(Zone::Hand, P0);
    let log_before = g.engine.log.len();
    g.act_holding(activate(ring, 3), &[]);
    g.cast(bounce, &[mtg_core::Target::Object(ring)]);
    // Bounce spends one card and returns the Ring; the pending ability cannot add a
    // counter to the departed Ring, but still draws for the one it had when it left.
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
    assert!(!g.engine.state.objects.contains_key(&ring));
    assert!(!g.engine.log[log_before..].iter().any(|e| matches!(e.event,
        mtg_core::Event::CountersChanged { object, .. } if object == ring)));
}

#[test]
fn returning_the_cast_ring_before_its_enter_trigger_resolves_still_grants_protection() {
    use mtg_engine::{actions::Action, choice::Answer};
    let mut t = Table::default();
    let ring_card = t.card("{4}", "Legendary Artifact", None, RING);
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target artifact to its owner's hand.",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let spell = g.put(ring_card, P0, Zone::Hand);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    let prompt = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            prompt.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    g.drive(
        |_, _| None,
        |g| {
            g.find(ring_card).is_some()
                && !g.stack().is_empty()
                && g.engine.state.priority == Some(P0)
        },
    );
    let ring = g.find(ring_card).unwrap();
    g.cast(bounce, &[mtg_core::Target::Object(ring)]);
    assert!(mtg_engine::targeting::player_protected(
        &g.engine.state,
        &g.table,
        P0
    ));
}
