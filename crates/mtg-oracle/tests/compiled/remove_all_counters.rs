use super::harness::*;
use mtg_core::{CounterKind, Event, Target, Zone};

#[test]
fn removes_each_permanents_actual_count_without_removing_other_kinds() {
    for targeted in [false, true] {
        let mut t = Table::default();
        let spell_card = t.card(
            "{0}",
            "Instant",
            None,
            if targeted {
                "Remove all +1/+1 counters from target creature."
            } else {
                "Remove all +1/+1 counters from all creatures."
            },
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        let creatures: Vec<_> = (0..3).map(|_| g.put(bear, P0, Zone::Battlefield)).collect();
        for (i, id) in creatures.iter().enumerate() {
            let counters = &mut g.engine.state.objects.get_mut(id).unwrap().counters;
            counters.insert(CounterKind::PlusOnePlusOne, i as i32 * 2);
            counters.insert(CounterKind::Shield, 1);
        }
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.main();
        let start = g.engine.log.len();
        let targets = if targeted {
            vec![Target::Object(creatures[1])]
        } else {
            vec![]
        };
        g.cast(spell, &targets);
        for (i, id) in creatures.iter().enumerate() {
            let counters = &g.engine.state.objects[id].counters;
            let expected = if targeted && i != 1 { i as i32 * 2 } else { 0 };
            assert_eq!(
                counters
                    .get(&CounterKind::PlusOnePlusOne)
                    .copied()
                    .unwrap_or(0),
                expected
            );
            assert_eq!(counters[&CounterKind::Shield], 1);
        }
        let deltas: Vec<_> = g.engine.log[start..]
            .iter()
            .filter_map(|e| match e.event {
                Event::CountersChanged {
                    object,
                    kind: CounterKind::PlusOnePlusOne,
                    delta,
                } => Some((object, delta)),
                _ => None,
            })
            .collect();
        let expected = if targeted {
            vec![(creatures[1], -2)]
        } else {
            vec![(creatures[1], -2), (creatures[2], -4)]
        };
        assert_eq!(deltas, expected);
    }
}

#[test]
fn end_step_trigger_removes_only_sources_counters() {
    let mut t = Table::default();
    let creature_card = t.card(
        "{0}",
        "Creature",
        Some((2, 2)),
        "At the beginning of your end step, remove all +1/+1 counters from this creature.",
    );
    let mut g = Game::new(t);
    let source = g.put(creature_card, P0, Zone::Battlefield);
    let other = g.put(creature_card, P1, Zone::Battlefield);
    for id in [source, other] {
        g.engine
            .state
            .objects
            .get_mut(&id)
            .unwrap()
            .counters
            .insert(CounterKind::PlusOnePlusOne, 3);
    }
    g.main();
    g.until(P0, mtg_core::Step::End);
    assert_eq!(g.pt(source), (2, 2));
    assert_eq!(g.pt(other), (5, 5));
}
