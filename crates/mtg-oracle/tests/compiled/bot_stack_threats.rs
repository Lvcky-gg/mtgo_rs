//! Counter selection against public stack threats and defenses.

use super::harness::*;
use mtg_core::{AbilityId, CardId, ObjectId, Zone};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, Choice, ChoiceKind},
};
use mtg_policy::bot::Bot;

fn opponent_priority(game: &mut Game) -> Choice {
    let choice = game.pending.take().unwrap();
    game.engine
        .answer(&game.table, choice.id, Answer::Pass)
        .unwrap();
    for _ in 0..100 {
        match game.engine.advance(&game.table) {
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended before response"),
            Progress::NeedsChoice(choice) => {
                assert_eq!(choice.who, P1);
                assert!(matches!(choice.kind, ChoiceKind::Priority { .. }));
                return choice;
            }
        }
    }
    panic!("no response window");
}

fn reply(game: &mut Game, choice: Choice, expected: Option<ObjectId>) {
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    let answer = bot.decide(&choice, &game.engine.view_for(P1));
    match expected {
        Some(id) => assert!(
            matches!(&answer, Answer::Action(Action::Cast { object }) if *object == id),
            "{answer:?}"
        ),
        None => assert!(matches!(&answer, Answer::Pass), "{answer:?}"),
    }
    game.engine.answer(&game.table, choice.id, answer).unwrap();
    game.drive(
        |game, choice| {
            Some(if matches!(choice.kind, ChoiceKind::Priority { .. }) {
                Answer::Pass
            } else {
                bot.decide(choice, &game.engine.view_for(choice.who))
            })
        },
        |game| game.stack().is_empty(),
    );
}

#[test]
fn stack_threat_uses_a_hard_counter_when_the_opponent_can_pay_the_tax() {
    for floating in [false, true] {
        let mut table = Table::default();
        let lethal = table.card("{0}", "Sorcery", None, "Target opponent loses 3 life.");
        let soft = table.card(
            "{0}",
            "Instant",
            None,
            "Counter target spell unless its controller pays {1}.",
        );
        let hard = table.card("{U}", "Instant", None, "Counter target spell.");
        let land = table.card("", "Land", None, "{T}: Add {U}.");
        let mut game = Game::new(table);
        let lethal = game.put(lethal, P0, Zone::Hand);
        let soft = game.put(soft, P1, Zone::Hand);
        let hard = game.put(hard, P1, Zone::Hand);
        game.put(land, P1, Zone::Battlefield);
        if !floating {
            game.put(land, P0, Zone::Battlefield);
        }
        game.engine.state.players.get_mut(&P1).unwrap().life = 3;
        game.main();
        game.act_holding(
            Action::Cast { object: lethal },
            &[mtg_core::Target::Player(P1)],
        );
        if floating {
            game.engine
                .state
                .players
                .get_mut(&P0)
                .unwrap()
                .mana
                .add(None, 1);
        }
        let choice = opponent_priority(&mut game);
        reply(&mut game, choice, Some(hard));
        assert_eq!(game.engine.state.player(P1).life, 3);
        assert_eq!(game.engine.state.objects[&soft].zone.zone, Zone::Hand);
    }
}

#[test]
fn stack_threat_holds_a_soft_counter_with_a_publicly_payable_tax() {
    let mut table = Table::default();
    let gain = table.card("{0}", "Sorcery", None, "You gain 3 life.");
    let soft = table.card(
        "{0}",
        "Instant",
        None,
        "Counter target spell unless its controller pays {1}.",
    );
    let mut game = Game::new(table);
    let gain = game.put(gain, P0, Zone::Hand);
    let soft = game.put(soft, P1, Zone::Hand);
    game.main();
    game.act_holding(Action::Cast { object: gain }, &[]);
    game.engine
        .state
        .players
        .get_mut(&P0)
        .unwrap()
        .mana
        .add(None, 1);
    let choice = opponent_priority(&mut game);
    reply(&mut game, choice, None);
    assert_eq!(game.engine.state.objects[&soft].zone.zone, Zone::Hand);
    assert_eq!(game.engine.state.player(P0).life, 23);
}

#[test]
fn stack_threat_holds_counters_against_visible_uncounterability() {
    for shield_owner in [None, Some(P0), Some(P1)] {
        let mut table = Table::default();
        let gain = table.card(
            "{0}",
            "Sorcery",
            None,
            if shield_owner.is_none() {
                "This spell can't be countered.\nYou gain 3 life."
            } else {
                "You gain 3 life."
            },
        );
        let counter = table.card("{0}", "Instant", None, "Counter target spell.");
        let shield = table.card(
            "{1}",
            "Creature",
            Some((1, 1)),
            "Spells you control can't be countered.",
        );
        let mut game = Game::new(table);
        let gain = game.put(gain, P0, Zone::Hand);
        let counter = game.put(counter, P1, Zone::Hand);
        if let Some(owner) = shield_owner {
            game.put(shield, owner, Zone::Battlefield);
        }
        game.main();
        game.act_holding(Action::Cast { object: gain }, &[]);
        let choice = opponent_priority(&mut game);
        let protected = shield_owner != Some(P1);
        reply(
            &mut game,
            choice,
            if protected { None } else { Some(counter) },
        );
        assert_eq!(
            game.engine.state.player(P0).life,
            if protected { 23 } else { 20 }
        );
        if protected {
            assert_eq!(game.engine.state.objects[&counter].zone.zone, Zone::Hand);
        }
    }
}

#[test]
fn stack_threat_matches_an_ability_counter_to_a_lethal_activated_ability() {
    let mut table = Table::default();
    let drain = table.card(
        "{1}",
        "Creature",
        Some((1, 1)),
        "{T}: Target opponent loses 3 life.",
    );
    let ordinary = table.card("{0}", "Instant", None, "Counter target spell.");
    let stifle = table.card(
        "{0}",
        "Instant",
        None,
        "Counter target activated or triggered ability.",
    );
    let mut game = Game::new(table);
    let drain = game.put(drain, P0, Zone::Battlefield);
    let ordinary = game.put(ordinary, P1, Zone::Hand);
    let stifle = game.put(stifle, P1, Zone::Hand);
    game.engine.state.players.get_mut(&P1).unwrap().life = 3;
    game.main();
    game.act_holding(
        Action::ActivateAbility {
            source: drain,
            ability: AbilityId(0),
        },
        &[mtg_core::Target::Player(P1)],
    );
    let choice = opponent_priority(&mut game);
    reply(&mut game, choice, Some(stifle));
    assert_eq!(game.engine.state.player(P1).life, 3);
    assert_eq!(game.engine.state.objects[&ordinary].zone.zone, Zone::Hand);
}

#[test]
fn stack_threat_does_not_confuse_an_ability_counter_with_a_spell_counter() {
    let mut table = Table::default();
    let gain = table.card("{0}", "Sorcery", None, "You gain 3 life.");
    let stifle = table.card(
        "{0}",
        "Instant",
        None,
        "Counter target activated or triggered ability.",
    );
    let mut game = Game::new(table);
    let gain = game.put(gain, P0, Zone::Hand);
    let stifle = game.put(stifle, P1, Zone::Hand);
    game.main();
    game.act_holding(Action::Cast { object: gain }, &[]);
    let choice = opponent_priority(&mut game);
    reply(&mut game, choice, None);
    assert_eq!(game.engine.state.objects[&stifle].zone.zone, Zone::Hand);
}

#[test]
fn stack_threat_uses_a_tax_counter_when_public_mana_is_insufficient() {
    // One permanent with two tapping mana abilities can still only pay one mana.
    let mut table = Table::default();
    let gain = table.card("{0}", "Sorcery", None, "You gain 3 life.");
    let soft = table.card(
        "{0}",
        "Instant",
        None,
        "Counter target spell unless its controller pays {2}.",
    );
    let land = table.card("", "Land", None, "{T}: Add {U}.\n{T}: Add {R}.");
    let mut game = Game::new(table);
    let gain = game.put(gain, P0, Zone::Hand);
    let soft = game.put(soft, P1, Zone::Hand);
    game.put(land, P0, Zone::Battlefield);
    game.main();
    game.act_holding(Action::Cast { object: gain }, &[]);
    let choice = opponent_priority(&mut game);
    reply(&mut game, choice, Some(soft));
    assert_eq!(game.engine.state.player(P0).life, 20);
}

#[test]
fn stack_threat_checks_the_actual_spell_type_instead_of_any_target_on_the_stack() {
    let mut table = Table::default();
    let gain = table.card("{0}", "Instant", None, "You gain 3 life.");
    let creature = table.card("{0}", "Creature", Some((1, 1)), "Flash");
    let narrow = table.card("{0}", "Instant", None, "Counter target noncreature spell.");
    let broad = table.card("{0}", "Instant", None, "Counter target spell.");
    let mut game = Game::new(table);
    let gain = game.put(gain, P0, Zone::Hand);
    let creature = game.put(creature, P0, Zone::Hand);
    let narrow = game.put(narrow, P1, Zone::Hand);
    let broad = game.put(broad, P1, Zone::Hand);
    game.main();
    game.act_holding(Action::Cast { object: gain }, &[]);
    game.act_holding(Action::Cast { object: creature }, &[]);
    let choice = opponent_priority(&mut game);
    // The narrow counter is offered because the life-gain spell underneath is
    // legal. It cannot answer the creature on top.
    let ChoiceKind::Priority { legal } = &choice.kind else {
        panic!("priority")
    };
    assert!(legal.actions.contains(&Action::Cast { object: narrow }));
    reply(&mut game, choice, Some(broad));
    assert!(!game.engine.state.objects.contains_key(&creature));
    assert_eq!(game.engine.state.objects[&narrow].zone.zone, Zone::Hand);
}

#[test]
fn stack_threat_does_not_cast_a_second_counter_over_its_own_response() {
    let mut table = Table::default();
    let gain = table.card("{0}", "Sorcery", None, "You gain 3 life.");
    let counter = table.card("{0}", "Instant", None, "Counter target spell.");
    let mut game = Game::new(table);
    let gain = game.put(gain, P0, Zone::Hand);
    game.put(counter, P1, Zone::Hand);
    let reserve = game.put(counter, P1, Zone::Hand);
    game.main();
    game.act_holding(Action::Cast { object: gain }, &[]);
    let choice = opponent_priority(&mut game);
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    let answer = bot.decide(&choice, &game.engine.view_for(P1));
    assert!(matches!(&answer, Answer::Action(Action::Cast { .. })));
    game.engine.answer(&game.table, choice.id, answer).unwrap();
    let mut responses = 1;
    game.drive(
        |game, choice| {
            if choice.who == P1 {
                let answer = bot.decide(choice, &game.engine.view_for(P1));
                if matches!(answer, Answer::Action(Action::Cast { .. })) {
                    responses += 1;
                }
                Some(answer)
            } else {
                Some(Answer::Pass)
            }
        },
        |game| game.stack().is_empty(),
    );
    assert_eq!(responses, 1);
    assert_eq!(game.engine.state.objects[&reserve].zone.zone, Zone::Hand);
    assert_eq!(game.engine.state.player(P0).life, 20);
}

#[test]
fn stack_threat_counter_keeps_the_lethal_top_spell_as_its_target() {
    let mut table = Table::default();
    let giant = table.card("{0}", "Creature", Some((8, 8)), "");
    let lethal = table.card("{0}", "Instant", None, "Target opponent loses 3 life.");
    let counter = table.card("{0}", "Instant", None, "Counter target spell.");
    let mut game = Game::new(table);
    let giant_card = giant;
    let giant = game.put(giant, P0, Zone::Hand);
    let lethal = game.put(lethal, P0, Zone::Hand);
    let counter = game.put(counter, P1, Zone::Hand);
    game.engine.state.players.get_mut(&P1).unwrap().life = 3;
    game.main();
    game.act_holding(Action::Cast { object: giant }, &[]);
    game.act_holding(
        Action::Cast { object: lethal },
        &[mtg_core::Target::Player(P1)],
    );
    let choice = opponent_priority(&mut game);
    reply(&mut game, choice, Some(counter));
    assert_eq!(game.engine.state.player(P1).life, 3);
    let giant = game.find(giant_card).unwrap();
    assert_eq!(
        game.engine.state.objects[&giant].zone.zone,
        Zone::Battlefield
    );
}

#[test]
fn stack_threat_preserves_the_last_counter_through_bait_then_stops_a_lethal_spell() {
    use mtg_policy::search::{Budget, Searcher};
    let mut table = Table::default();
    let bait = table.card("{0}", "Instant", None, "You gain 3 life.");
    let lethal = table.card("{0}", "Instant", None, "Target opponent loses 3 life.");
    let counter = table.card("{0}", "Instant", None, "Counter target spell.");
    let mut game = Game::new(table);
    let bait = game.put(bait, P0, Zone::Hand);
    let lethal = game.put(lethal, P0, Zone::Hand);
    let counter = game.put(counter, P1, Zone::Hand);
    game.engine.state.players.get_mut(&P0).unwrap().life = 30;
    game.engine.state.players.get_mut(&P1).unwrap().life = 3;
    game.main();
    game.act_holding(Action::Cast { object: bait }, &[]);
    let choice = opponent_priority(&mut game);
    let searcher = Searcher::new(
        Bot::new(&game.table, (0..32).map(CardId)),
        Budget {
            playouts: 12,
            horizon_turns: 0,
            max_choices: 40,
            ..Default::default()
        },
        41,
    );
    assert!(matches!(
        searcher.decide(&game.engine, &game.table, &choice),
        Answer::Pass
    ));
    reply(&mut game, choice, None);
    assert_eq!(game.engine.state.player(P0).life, 33);
    assert_eq!(game.engine.state.objects[&counter].zone.zone, Zone::Hand);

    game.act_holding(
        Action::Cast { object: lethal },
        &[mtg_core::Target::Player(P1)],
    );
    let choice = opponent_priority(&mut game);
    reply(&mut game, choice, Some(counter));
    assert_eq!(game.engine.state.player(P1).life, 3);
    assert!(!game.engine.state.player(P1).has_lost);
}

#[test]
fn stack_threat_still_answers_life_gain_when_life_is_scarce_or_the_swing_is_large() {
    for (initial, text) in [
        (3, "You gain 3 life."),
        (30, "You gain 12 life."),
        (30, "You gain 2 life, then you gain 2 life."),
    ] {
        let mut table = Table::default();
        let gain = table.card("{0}", "Instant", None, text);
        let counter = table.card("{0}", "Instant", None, "Counter target spell.");
        let mut game = Game::new(table);
        let gain = game.put(gain, P0, Zone::Hand);
        let counter = game.put(counter, P1, Zone::Hand);
        game.engine.state.players.get_mut(&P0).unwrap().life = initial;
        game.main();
        game.act_holding(Action::Cast { object: gain }, &[]);
        let choice = opponent_priority(&mut game);
        reply(&mut game, choice, Some(counter));
        assert_eq!(game.engine.state.player(P0).life, initial);
    }
}

#[test]
fn stack_threat_recognizes_that_small_life_gain_can_feed_a_lethal_engine() {
    let mut table = Table::default();
    let gain = table.card("{0}", "Instant", None, "You gain 3 life.");
    let payoff = table.card(
        "{1}",
        "Creature",
        Some((1, 1)),
        "Whenever you gain life, target opponent loses that much life.",
    );
    let counter = table.card("{0}", "Instant", None, "Counter target spell.");
    let mut game = Game::new(table);
    let gain = game.put(gain, P0, Zone::Hand);
    game.put(payoff, P0, Zone::Battlefield);
    let counter = game.put(counter, P1, Zone::Hand);
    game.engine.state.players.get_mut(&P0).unwrap().life = 30;
    game.engine.state.players.get_mut(&P1).unwrap().life = 3;
    game.main();
    game.act_holding(Action::Cast { object: gain }, &[]);
    let choice = opponent_priority(&mut game);
    reply(&mut game, choice, Some(counter));
    assert_eq!(game.engine.state.player(P0).life, 30);
    assert_eq!(game.engine.state.player(P1).life, 3);
}

#[test]
fn stack_threat_still_answers_life_gain_that_can_take_an_opponent_out_of_lethal_range() {
    let mut table = Table::default();
    let gain = table.card("{0}", "Instant", None, "You gain 3 life.");
    let counter = table.card("{0}", "Instant", None, "Counter target spell.");
    let attacker = table.card("{9}", "Creature", Some((30, 30)), "");
    let mut game = Game::new(table);
    let gain = game.put(gain, P0, Zone::Hand);
    let counter = game.put(counter, P1, Zone::Hand);
    game.put(attacker, P1, Zone::Battlefield);
    game.engine.state.players.get_mut(&P0).unwrap().life = 30;
    game.main();
    game.act_holding(Action::Cast { object: gain }, &[]);
    let choice = opponent_priority(&mut game);
    reply(&mut game, choice, Some(counter));
    assert_eq!(game.engine.state.player(P0).life, 30);
}

#[test]
fn stack_threat_does_not_dismiss_a_mixed_draw_spell_as_small_life_gain() {
    let mut table = Table::default();
    let gain = table.card("{0}", "Instant", None, "You gain 3 life. Draw three cards.");
    let counter = table.card("{0}", "Instant", None, "Counter target spell.");
    let mut game = Game::new(table);
    let gain = game.put(gain, P0, Zone::Hand);
    let counter = game.put(counter, P1, Zone::Hand);
    game.engine.state.players.get_mut(&P0).unwrap().life = 30;
    game.main();
    let hand = game.count(Zone::Hand, P0);
    game.act_holding(Action::Cast { object: gain }, &[]);
    let choice = opponent_priority(&mut game);
    reply(&mut game, choice, Some(counter));
    assert_eq!(game.engine.state.player(P0).life, 30);
    assert_eq!(game.count(Zone::Hand, P0), hand - 1);
}

#[test]
fn stack_threat_respects_life_as_a_resource_and_alternate_win_requirement() {
    for text in [
        "Pay 1 life: Draw a card.",
        "At the beginning of your end step, if you gained life this turn, draw a card.",
        "At the beginning of your end step, if you have 33 or more life, you win the game.",
    ] {
        let mut table = Table::default();
        let gain = table.card("{0}", "Instant", None, "You gain 3 life.");
        let payoff = table.card("{1}", "Enchantment", None, text);
        let counter = table.card("{0}", "Instant", None, "Counter target spell.");
        let mut game = Game::new(table);
        let gain = game.put(gain, P0, Zone::Hand);
        game.put(payoff, P0, Zone::Battlefield);
        let counter = game.put(counter, P1, Zone::Hand);
        game.engine.state.players.get_mut(&P0).unwrap().life = 30;
        game.main();
        game.act_holding(Action::Cast { object: gain }, &[]);
        let choice = opponent_priority(&mut game);
        reply(&mut game, choice, Some(counter));
        assert_eq!(game.engine.state.player(P0).life, 30);
    }
}
