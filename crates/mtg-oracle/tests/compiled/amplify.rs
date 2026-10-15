use super::harness::*;
use mtg_core::{CounterKind, Event, Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};
use std::cell::Cell;

#[test]
fn amplify_offers_only_your_matching_creature_cards_and_counts_each_card_once() {
    let mut t = Table::default();
    let warhawk = t.card(
        "{0}",
        "Creature — Bird Soldier",
        Some((3, 3)),
        "Amplify 1\nFlying",
    );
    let bird = t.card("{0}", "Creature — Bird", Some((1, 1)), "");
    let soldier = t.card("{0}", "Creature — Soldier", Some((1, 1)), "");
    let both = t.card("{0}", "Creature — Bird Soldier", Some((1, 1)), "");
    let changeling = t.card("{0}", "Creature — Elf", Some((1, 1)), "Changeling");
    let unrelated = t.card("{0}", "Creature — Elf", Some((1, 1)), "");
    let kindred = t.card("{0}", "Kindred Enchantment — Bird", None, "");
    let mut g = Game::new(t);
    let spell = g.put(warhawk, P0, Zone::Hand);
    let reveal: Vec<_> = [bird, soldier, both, changeling]
        .into_iter()
        .map(|card| g.put(card, P0, Zone::Hand))
        .collect();
    g.put(bird, P1, Zone::Hand);
    g.put(unrelated, P0, Zone::Hand);
    g.put(kindred, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let asked = Cell::new(false);
    g.drive(
        |_, choice| {
            if let ChoiceKind::ChooseObjects { from, min, max } = &choice.kind {
                assert_eq!((*min, *max), (0, 4));
                assert_eq!(from.len(), 4);
                assert!(reveal.iter().all(|id| from.contains(id)));
                asked.set(true);
                return Some(Answer::Objects(reveal.clone()));
            }
            None
        },
        |g| asked.get() && g.stack().is_empty() && g.engine.state.priority == Some(P0),
    );
    let warhawk = g.find(warhawk).unwrap();
    assert_eq!(g.pt(warhawk), (7, 7));
    assert_eq!(g.count(Zone::Hand, P0), hand - 1);
    assert!(
        reveal
            .iter()
            .all(|id| g.engine.state.objects[id].zone.zone == Zone::Hand)
    );
    assert_eq!(
        g.engine
            .log
            .iter()
            .filter(|e| matches!(e.event, Event::Revealed { object } if reveal.contains(&object)))
            .count(),
        4
    );
}

#[test]
fn amplify_three_places_six_counters_for_two_revealed_dragons_before_etb_triggers() {
    let mut t = Table::default();
    let dragon = t.card(
        "{0}",
        "Creature — Dragon",
        Some((0, 0)),
        "Amplify 3\nWhen this creature enters, you gain life equal to this creature's power.",
    );
    let other = t.card("{0}", "Creature — Dragon", Some((1, 1)), "");
    let mut g = Game::new(t);
    let spell = g.put(dragon, P0, Zone::Hand);
    let reveal = [g.put(other, P0, Zone::Hand), g.put(other, P0, Zone::Hand)];
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(reveal.to_vec())],
    );
    let dragon = g.find(dragon).unwrap();
    assert_eq!(g.pt(dragon), (6, 6));
    assert_eq!(g.life(P0), 26);
    assert_eq!(
        g.engine.state.objects[&dragon].counters[&CounterKind::PlusOnePlusOne],
        6
    );
}

#[test]
fn returning_and_recasting_an_amplify_creature_asks_again_and_can_reveal_none() {
    let mut t = Table::default();
    let crawler = t.card("{0}", "Creature — Elf", Some((2, 2)), "Amplify 1");
    let other = t.card("{0}", "Creature — Elf", Some((1, 1)), "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let spell = g.put(crawler, P0, Zone::Hand);
    let reveal = g.put(other, P0, Zone::Hand);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![reveal])],
    );
    let object = g.find(crawler).unwrap();
    assert_eq!(g.pt(object), (3, 3));
    g.cast(bounce, &[Target::Object(object)]);
    let spell = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == crawler && o.zone.zone == Zone::Hand)
        .unwrap()
        .id;
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![])],
    );
    let object = g.find(crawler).unwrap();
    assert_eq!(g.pt(object), (2, 2));
    assert!(g.engine.state.objects[&object].counters.is_empty());
}
