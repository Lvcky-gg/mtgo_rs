use super::harness::*;
use mtg_core::Zone;

#[test]
fn self_attacking_alone_pumps_only_the_lone_attacker() {
    for alone in [false, true] {
        let mut t = Table::default();
        let attacker_card = t.card(
            "{1}{R}",
            "Creature",
            Some((2, 2)),
            "Whenever this creature attacks alone, it gets +2/+0 until end of turn.",
        );
        let helper_card = t.card("{R}", "Creature", Some((1, 1)), "");
        let mut g = Game::new(t);
        let attacker = g.put(attacker_card, P0, Zone::Battlefield);
        let helper = g.put(helper_card, P0, Zone::Battlefield);
        g.main();
        let attack = if alone {
            vec![attacker]
        } else {
            vec![attacker, helper]
        };
        g.combat(&attack, &[], &[], &[]);
        assert_eq!(g.pt(attacker), if alone { (4, 2) } else { (2, 2) });
        assert_eq!(g.life(P1), if alone { 16 } else { 17 });
    }
}

#[test]
fn self_attacking_alone_creates_food_only_for_its_own_attack() {
    for attack_self in [false, true] {
        let mut t = Table::default();
        let creature = t.card(
            "{G}",
            "Creature",
            Some((1, 1)),
            "Whenever this creature attacks alone, create a Food token.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        let creature = g.put(creature, P0, Zone::Battlefield);
        let helper = g.put(bear, P0, Zone::Battlefield);
        g.main();
        g.combat(
            &[if attack_self { creature } else { helper }],
            &[],
            &[],
            &[],
        );
        assert_eq!(
            g.engine
                .state
                .battlefield()
                .iter()
                .filter(|id| g.engine.state.objects[id].is_token)
                .count(),
            usize::from(attack_self)
        );
    }
}

#[test]
fn generic_attacks_alone_triggers_only_with_a_single_attacker() {
    for alone in [false, true] {
        let mut t = Table::default();
        let enchantment = t.card(
            "{G}",
            "Enchantment",
            None,
            "Whenever a creature you control attacks alone, you gain 1 life.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.put(enchantment, P0, Zone::Battlefield);
        let first = g.put(bear, P0, Zone::Battlefield);
        let second = g.put(bear, P0, Zone::Battlefield);
        g.main();
        let attack = if alone {
            vec![first]
        } else {
            vec![first, second]
        };
        g.combat(&attack, &[], &[], &[]);
        assert_eq!(g.life(P0), if alone { 21 } else { 20 });
    }
}

#[test]
fn an_alone_trigger_still_resolves_after_another_creature_is_put_attacking() {
    use mtg_core::Target;
    use mtg_engine::{
        Progress,
        actions::Action,
        choice::{Answer, ChoiceKind},
    };
    let mut t = Table::default();
    let attacker_card = t.card(
        "{G}",
        "Creature",
        Some((2, 2)),
        "Exalted\nWhenever this creature attacks alone, you gain 1 life.",
    );
    let helper_card = t.bear();
    let response_card = t.card("{0}", "Instant", None, "Draw a card.");
    let mut g = Game::new(t);
    let attacker = g.put(attacker_card, P0, Zone::Battlefield);
    let helper = g.put(helper_card, P0, Zone::Battlefield);
    let response = g.put(response_card, P0, Zone::Hand);
    g.main();
    let priority = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, priority.id, Answer::Pass)
        .unwrap();
    for _ in 0..1000 {
        if let Progress::NeedsChoice(choice) = g.engine.advance(&g.table) {
            if matches!(choice.kind, ChoiceKind::Priority { .. })
                && choice.who == P0
                && !g.stack().is_empty()
            {
                // Simulate another creature entering combat after the attack triggers
                // have been put on the stack (it was not declared as an attacker).
                g.engine
                    .state
                    .combat
                    .attackers
                    .insert(helper, Target::Player(P1));
                g.pending = Some(choice);
                g.act(Action::Cast { object: response }, &[], &[]);
                assert_eq!(g.life(P0), 21);
                assert_eq!(
                    g.pt(attacker),
                    (3, 3),
                    "exalted also checks the attack event only"
                );
                return;
            }
            let answer = match &choice.kind {
                ChoiceKind::DeclareAttackers { .. } => Answer::Objects(vec![attacker]),
                ChoiceKind::OrderTriggers { triggers, .. } => Answer::Order(triggers.clone()),
                _ => choice.default.clone().unwrap_or(Answer::Pass),
            };
            g.engine.answer(&g.table, choice.id, answer).unwrap();
        }
    }
    panic!("the attack trigger never reached the stack");
}
