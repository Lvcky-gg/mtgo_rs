use super::harness::*;
use mtg_core::{Cause, Event, Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn reveal_or_mana_cost_uses_only_the_selected_payment() {
    for (lands, has_match, pay_mana) in [
        (2, true, false),
        (3, false, true),
        (3, true, false),
        (3, true, true),
    ] {
        let mut t = Table::default();
        let spell_card = t.card("{1}{G}", "Sorcery", None,
            "As an additional cost to cast this spell, reveal a Dinosaur card from your hand or pay {1}.\nDraw two cards.");
        let dinosaur = t.card("{G}", "Creature — Dinosaur", Some((1, 1)), "");
        let mut g = Game::new(t);
        g.lands(lands);
        let reveal = has_match.then(|| g.put(dinosaur, P0, Zone::Hand));
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.main();
        let before = g.count(Zone::Hand, P0);
        let log_start = g.engine.log.len();
        let mut answers = Vec::new();
        if lands == 3 && has_match {
            answers.push(Answer::Modes(vec![u8::from(pay_mana)]));
        }
        if !pay_mana {
            answers.push(Answer::Objects(vec![reveal.unwrap()]));
        }
        g.act(Action::Cast { object: spell }, &[], &answers);
        assert_eq!(g.count(Zone::Hand, P0), before + 1);
        if let Some(reveal) = reveal {
            assert_eq!(
                g.engine.state.objects[&reveal].zone,
                ZoneRef::of(Zone::Hand, P0)
            );
        }
        let revealed: Vec<_> = g.engine.log[log_start..]
            .iter()
            .filter_map(|e| match e.event {
                Event::Revealed { object } => {
                    assert!(matches!(e.cause, Cause::CostPayment(_)));
                    Some(object)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            revealed,
            if pay_mana {
                vec![]
            } else {
                vec![reveal.unwrap()]
            }
        );
        let used = g
            .engine
            .state
            .battlefield()
            .iter()
            .filter(|id| g.engine.state.objects[id].tapped)
            .count();
        assert_eq!(used, if pay_mana { 3 } else { 2 });
    }
}

#[test]
fn reveal_cannot_use_the_spell_itself_or_an_opponents_hand() {
    for extra_match in [false, true] {
        let mut t = Table::default();
        let stalwart = t.card("{W}", "Creature — Kithkin", Some((2, 2)),
            "As an additional cost to cast this spell, reveal a Kithkin card from your hand or pay {3}.");
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(stalwart, P0, Zone::Hand);
        g.put(stalwart, P1, Zone::Hand);
        if extra_match {
            g.put(stalwart, P0, Zone::Hand);
        }
        let actions = g.main();
        assert_eq!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Cast { object } if *object == spell)),
            extra_match
        );
    }
}

#[test]
fn revealing_multiple_cards_happens_during_casting_without_moving_them() {
    let mut t = Table::default();
    let spell_card = t.card("{0}", "Instant", None,
        "As an additional cost to cast this spell, reveal two blue cards from your hand.\nDraw a card.");
    let blue_card = t.card("{U}", "Creature", Some((1, 1)), "");
    let red_card = t.card("{R}", "Creature", Some((1, 1)), "");
    let mut g = Game::new(t);
    let blue: Vec<_> = (0..2).map(|_| g.put(blue_card, P0, Zone::Hand)).collect();
    g.put(red_card, P0, Zone::Hand);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    let library = g.count(Zone::Library, P0);
    let start = g.engine.log.len();
    g.act_holding(Action::Cast { object: spell }, &[]);
    assert_eq!(g.count(Zone::Library, P0), library);
    for object in &blue {
        assert_eq!(
            g.engine.state.objects[object].zone,
            ZoneRef::of(Zone::Hand, P0)
        );
    }
    let revealed: Vec<_> = g.engine.log[start..]
        .iter()
        .filter_map(|e| match e.event {
            Event::Revealed { object } => Some(object),
            _ => None,
        })
        .collect();
    assert_eq!(revealed, blue);
}

#[test]
fn printed_reveal_costs_compile() {
    let mut t = Table::default();
    t.named_card("Thunderherd Migration", "{1}{G}", "Sorcery", None,
        "As an additional cost to cast this spell, reveal a Dinosaur card from your hand or pay {1}.\nSearch your library for a basic land card, put it onto the battlefield tapped, then shuffle.");
    t.named_card("Goldmeadow Stalwart", "{W}", "Creature — Kithkin Soldier", Some((2, 2)),
        "As an additional cost to cast this spell, reveal a Kithkin card from your hand or pay {3}.");
}
