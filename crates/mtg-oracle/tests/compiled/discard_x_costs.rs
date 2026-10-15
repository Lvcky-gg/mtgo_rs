use super::harness::*;
use mtg_core::{Event, Target, Zone};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
};

#[test]
fn sickening_dreams_discards_the_chosen_cards_before_dealing_x_damage() {
    let mut t = Table::default();
    let dreams = t.card("{0}", "Sorcery", None, "As an additional cost to cast this spell, discard X cards.\n~ deals X damage to each creature and each player.");
    let bear = t.bear();
    let large = t.card("{0}", "Creature", Some((4, 4)), "");
    let mut g = Game::new(t);
    let dreams = g.put(dreams, P0, Zone::Hand);
    let discarded = [g.put(bear, P0, Zone::Hand), g.put(bear, P0, Zone::Hand)];
    let small = g.put(bear, P1, Zone::Battlefield);
    let large = g.put(large, P1, Zone::Battlefield);
    g.main();
    g.act(
        Action::Cast { object: dreams },
        &[],
        &[Answer::Number(2), Answer::Objects(discarded.to_vec())],
    );
    assert_eq!([g.life(P0), g.life(P1)], [18, 18]);
    assert!(!g.engine.state.objects.contains_key(&small));
    assert_eq!(g.engine.state.objects[&large].damage, 2);
    let discards: Vec<_> = g
        .engine
        .log
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.event, Event::ZoneChange { from, to, .. } if from.zone == Zone::Hand && to.zone == Zone::Graveyard))
        .collect();
    assert_eq!(discards.len(), 2);
    let first_damage = g
        .engine
        .log
        .iter()
        .position(|e| {
            matches!(
                e.event,
                Event::DamageMarked { .. } | Event::DamageDealtToPlayer { .. }
            )
        })
        .unwrap();
    assert!(discards.iter().all(|(index, _)| *index < first_damage));
}

#[test]
fn x_is_bounded_by_other_hand_cards_and_by_variable_mana_when_present() {
    for (cost, lands, expected_max) in [("{0}", 0, 3), ("{X}", 1, 1), ("{X}", 10, 3)] {
        let mut t = Table::default();
        let spell = t.card(
            cost,
            "Sorcery",
            None,
            "As an additional cost to cast this spell, discard X cards.\nYou gain X life.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(lands);
        let spell = g.put(spell, P0, Zone::Hand);
        g.put(bear, P0, Zone::Hand);
        g.put(bear, P0, Zone::Hand);
        g.main();
        let priority = g.pending.take().unwrap();
        g.engine
            .answer(
                &g.table,
                priority.id,
                Answer::Action(Action::Cast { object: spell }),
            )
            .unwrap();
        let choice = loop {
            match g.engine.advance(&g.table) {
                Progress::Continue => {}
                Progress::NeedsChoice(c) => break c,
                other => panic!("{other:?}"),
            }
        };
        assert!(
            matches!(choice.kind, ChoiceKind::ChooseX { min: 0, max } if max == expected_max),
            "{cost}: {:?}",
            choice.kind
        );
        assert!(
            g.engine
                .answer(&g.table, choice.id, Answer::Number(expected_max + 1))
                .is_err()
        );
        g.engine
            .answer(&g.table, choice.id, Answer::Number(0))
            .unwrap();
        g.drive(
            |_, _| None,
            |g| g.stack().is_empty() && g.engine.state.priority == Some(P0),
        );
        assert_eq!(g.life(P0), 20);
        assert!(
            !g.engine
                .log
                .iter()
                .any(|e| matches!(e.event, Event::ZoneChange { from, to, .. } if from.zone == Zone::Hand && to.zone == Zone::Graveyard))
        );
    }
}

#[test]
fn channeled_force_reuses_the_announced_x_for_draw_and_targeted_damage() {
    let mut t = Table::default();
    let force = t.card("{0}", "Instant", None, "As an additional cost to cast this spell, discard X cards.\nTarget player draws X cards. ~ deals X damage to up to one target creature or planeswalker.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let force = g.put(force, P0, Zone::Hand);
    let discard = g.put(bear, P0, Zone::Hand);
    let creature = g.put(bear, P1, Zone::Battlefield);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: force },
        &[Target::Player(P0), Target::Object(creature)],
        &[Answer::Number(1), Answer::Objects(vec![discard])],
    );
    assert_eq!(g.count(Zone::Hand, P0), hand - 1);
    assert_eq!(g.engine.state.objects[&creature].damage, 1);
}
