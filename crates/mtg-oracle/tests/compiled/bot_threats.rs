//! Threat assessment and response selection through compiled cards and real choices.

use super::harness::*;
use mtg_core::{CardId, ObjectId, Step, Target, Zone};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
};
use mtg_policy::bot::Bot;

fn cast_with_bot(game: &mut Game, spell: ObjectId) -> Vec<Target> {
    cast_with_strategy(game, spell, false)
}

fn cast_with_strategy(game: &mut Game, spell: ObjectId, search: bool) -> Vec<Target> {
    use mtg_policy::search::{Budget, Searcher};
    game.main();
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    let searcher = Searcher::new(
        bot,
        Budget {
            playouts: 12,
            horizon_turns: 0,
            max_choices: 40,
            ..Default::default()
        },
        29,
    );
    let choice = game.pending.take().unwrap();
    game.engine
        .answer(
            &game.table,
            choice.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let mut selected = Vec::new();
    for _ in 0..200 {
        match game.engine.advance(&game.table) {
            Progress::Continue => {}
            Progress::GameOver { .. } => return selected,
            Progress::NeedsChoice(choice) => {
                if matches!(choice.kind, ChoiceKind::Priority { .. }) && game.stack().is_empty() {
                    return selected;
                }
                let answer = if matches!(choice.kind, ChoiceKind::Priority { .. }) {
                    Answer::Pass
                } else {
                    if search {
                        searcher.decide(&game.engine, &game.table, &choice)
                    } else {
                        searcher
                            .bot
                            .decide(&choice, &game.engine.view_for(choice.who))
                    }
                };
                if let Answer::Targets(picks) = &answer {
                    selected.extend(picks.iter().flatten().copied());
                }
                game.engine.answer(&game.table, choice.id, answer).unwrap();
            }
        }
    }
    panic!("spell did not resolve");
}

#[test]
fn threat_removal_prefers_a_cheap_repeatable_draw_engine_to_an_expensive_vanilla() {
    for search in [false, true] {
        let mut table = Table::default();
        let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
        let engine = table.card(
            "{U}",
            "Creature",
            Some((1, 1)),
            "At the beginning of your upkeep, draw a card.",
        );
        let vanilla = table.card("{6}", "Creature", Some((6, 6)), "");
        let mut game = Game::new(table);
        let spell = game.put(removal, P0, Zone::Hand);
        let engine = game.put(engine, P1, Zone::Battlefield);
        let vanilla = game.put(vanilla, P1, Zone::Battlefield);
        assert_eq!(
            cast_with_strategy(&mut game, spell, search),
            vec![Target::Object(engine)]
        );
        assert!(!game.engine.state.objects.contains_key(&engine));
        assert_eq!(game.count(Zone::Graveyard, P1), 1);
        assert_eq!(
            game.engine.state.objects[&vanilla].zone.zone,
            Zone::Battlefield
        );
    }
}

#[test]
fn threat_burn_kills_a_smaller_creature_instead_of_wounding_a_large_one() {
    let mut table = Table::default();
    let burn = table.card("{0}", "Instant", None, "~ deals 3 damage to any target.");
    let giant = table.card("{8}", "Creature", Some((8, 8)), "");
    let small = table.bear();
    let mut game = Game::new(table);
    let spell = game.put(burn, P0, Zone::Hand);
    let giant = game.put(giant, P1, Zone::Battlefield);
    let small = game.put(small, P1, Zone::Battlefield);
    assert_eq!(cast_with_bot(&mut game, spell), vec![Target::Object(small)]);
    assert!(!game.engine.state.objects.contains_key(&small));
    assert_eq!(game.count(Zone::Graveyard, P1), 1);
    assert_eq!(game.engine.state.objects[&giant].damage, 0);
}

#[test]
fn threat_burn_takes_a_win_over_killing_a_creature() {
    let mut table = Table::default();
    let burn = table.card("{0}", "Instant", None, "~ deals 3 damage to any target.");
    let small = table.bear();
    let mut game = Game::new(table);
    let spell = game.put(burn, P0, Zone::Hand);
    game.put(small, P1, Zone::Battlefield);
    game.engine.state.players.get_mut(&P1).unwrap().life = 3;
    assert_eq!(cast_with_bot(&mut game, spell), vec![Target::Player(P1)]);
    assert!(game.engine.state.player(P1).has_lost);
}

#[test]
fn threat_destroy_avoids_indestructible_but_exile_answers_it() {
    for exile in [false, true] {
        let mut table = Table::default();
        let removal = table.card(
            "{0}",
            "Instant",
            None,
            if exile {
                "Exile target creature."
            } else {
                "Destroy target creature."
            },
        );
        let immortal = table.card("{7}", "Creature", Some((7, 7)), "Indestructible");
        let small = table.bear();
        let mut game = Game::new(table);
        let spell = game.put(removal, P0, Zone::Hand);
        let immortal = game.put(immortal, P1, Zone::Battlefield);
        let small = game.put(small, P1, Zone::Battlefield);
        let target = if exile { immortal } else { small };
        assert_eq!(
            cast_with_bot(&mut game, spell),
            vec![Target::Object(target)]
        );
        assert!(!game.engine.state.objects.contains_key(&target));
        assert_eq!(
            game.engine
                .state
                .objects
                .values()
                .filter(|o| o.owner == P1
                    && o.zone.zone == if exile { Zone::Exile } else { Zone::Graveyard })
                .count(),
            1
        );
    }
}

#[test]
fn threat_combat_response_uses_effective_removal_before_lethal_damage() {
    let mut table = Table::default();
    let burn = table.card(
        "{0}",
        "Instant",
        None,
        "~ deals 2 damage to target creature.",
    );
    let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
    let attacker = table.card("{1}", "Creature", Some((3, 3)), "");
    let bait = table.card("{7}", "Creature", Some((1, 1)), "");
    let mut game = Game::new(table);
    game.put(burn, P1, Zone::Hand);
    let removal = game.put(removal, P1, Zone::Hand);
    let attacker = game.put(attacker, P0, Zone::Battlefield);
    let bait = game.put(bait, P0, Zone::Battlefield);
    game.engine.state.players.get_mut(&P1).unwrap().life = 3;
    game.main();
    let choice = game.pending.take().unwrap();
    game.engine
        .answer(&game.table, choice.id, Answer::Pass)
        .unwrap();
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    let mut responded = false;
    game.drive(
        |game, choice| {
            if matches!(choice.kind, ChoiceKind::DeclareAttackers { .. }) {
                return Some(Answer::Attackers(vec![
                    (attacker, Target::Player(P1)),
                    (bait, Target::Player(P1)),
                ]));
            }
            if choice.who == P1 {
                let answer = bot.decide(choice, &game.engine.view_for(P1));
                if let Answer::Action(Action::Cast { object }) = &answer {
                    // Burn is useful against the bait, but removal must answer the
                    // larger attacker first to keep the player alive.
                    if !responded {
                        assert_eq!(*object, removal);
                    }
                    responded = true;
                }
                Some(answer)
            } else {
                Some(Answer::Pass)
            }
        },
        |game| game.engine.state.step == Step::PostcombatMain && game.stack().is_empty(),
    );
    assert!(responded);
    assert!(!game.engine.state.player(P1).has_lost);
    assert!(!game.engine.state.objects.contains_key(&attacker));
    assert!(game.count(Zone::Graveyard, P0) >= 1);
}

#[test]
fn threat_holds_removal_that_cannot_answer_any_opponent() {
    for indestructible in [false, true] {
        let mut table = Table::default();
        let removal = table.card(
            "{0}",
            "Instant",
            None,
            if indestructible {
                "Destroy target creature."
            } else {
                "~ deals 2 damage to target creature."
            },
        );
        let giant = table.card(
            "{7}",
            "Creature",
            Some((7, 7)),
            if indestructible { "Indestructible" } else { "" },
        );
        let mut game = Game::new(table);
        let spell = game.put(removal, P0, Zone::Hand);
        game.put(giant, P1, Zone::Battlefield);
        game.main();
        let bot = Bot::new(&game.table, (0..32).map(CardId));
        let choice = game.pending.as_ref().unwrap();
        assert!(matches!(
            bot.decide(choice, &game.engine.view_for(P0)),
            Answer::Pass
        ));
        assert_eq!(game.engine.state.objects[&spell].zone.zone, Zone::Hand);
    }
}

#[test]
fn threat_does_not_treat_spent_etb_card_draw_as_a_repeatable_engine() {
    let mut table = Table::default();
    let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
    let spent = table.card(
        "{1}",
        "Creature",
        Some((1, 1)),
        "When ~ enters, draw three cards.",
    );
    let vanilla = table.card("{6}", "Creature", Some((6, 6)), "");
    let mut game = Game::new(table);
    let spell = game.put(removal, P0, Zone::Hand);
    game.put(spent, P1, Zone::Battlefield);
    let vanilla = game.put(vanilla, P1, Zone::Battlefield);
    assert_eq!(
        cast_with_bot(&mut game, spell),
        vec![Target::Object(vanilla)]
    );
}

#[test]
fn threat_does_not_spend_narrow_removal_on_an_unrelated_permanent_during_combat() {
    let mut table = Table::default();
    let removal = table.card("{0}", "Instant", None, "Destroy target blue creature.");
    let attacker = table.card("{G}", "Creature", Some((3, 3)), "");
    let blue = table.card("{U}", "Creature", Some((1, 1)), "");
    let mut game = Game::new(table);
    let spell = game.put(removal, P1, Zone::Hand);
    let attacker = game.put(attacker, P0, Zone::Battlefield);
    game.put(blue, P0, Zone::Battlefield);
    game.main();
    let choice = game.pending.take().unwrap();
    game.engine
        .answer(&game.table, choice.id, Answer::Pass)
        .unwrap();
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    game.drive(
        |game, choice| {
            if matches!(choice.kind, ChoiceKind::DeclareAttackers { .. }) {
                Some(Answer::Attackers(vec![(attacker, Target::Player(P1))]))
            } else if choice.who == P1 {
                Some(bot.decide(choice, &game.engine.view_for(P1)))
            } else {
                Some(Answer::Pass)
            }
        },
        |game| game.engine.state.step == Step::PostcombatMain && game.stack().is_empty(),
    );
    assert_eq!(game.engine.state.objects[&spell].zone.zone, Zone::Hand);
    assert_eq!(game.engine.state.player(P1).life, 17);
}

#[test]
fn threat_counter_uses_the_cheaper_effective_response() {
    let mut table = Table::default();
    let draw = table.card("{0}", "Sorcery", None, "Draw a card.");
    let expensive = table.card("{1}{U}", "Instant", None, "Counter target spell.");
    let cheap = table.card("{U}", "Instant", None, "Counter target spell.");
    let land = table.card("", "Land", None, "{T}: Add {U}.");
    let mut game = Game::new(table);
    let draw = game.put(draw, P0, Zone::Hand);
    game.put(expensive, P1, Zone::Hand);
    let cheap = game.put(cheap, P1, Zone::Hand);
    for _ in 0..2 {
        game.put(land, P1, Zone::Battlefield);
    }
    game.main();
    game.act_holding(Action::Cast { object: draw }, &[]);
    let choice = game.pending.take().unwrap();
    game.engine
        .answer(&game.table, choice.id, Answer::Pass)
        .unwrap();
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    for _ in 0..100 {
        match game.engine.advance(&game.table) {
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended before response"),
            Progress::NeedsChoice(choice) => {
                assert_eq!(choice.who, P1);
                let answer = bot.decide(&choice, &game.engine.view_for(P1));
                assert!(
                    matches!(&answer, Answer::Action(Action::Cast { object }) if *object == cheap),
                    "{answer:?}"
                );
                game.engine.answer(&game.table, choice.id, answer).unwrap();
                return;
            }
        }
    }
    panic!("no response window");
}

#[test]
fn threat_saves_the_player_instead_of_answering_a_larger_walker_attacker() {
    let mut table = Table::default();
    let removal = table.card("{1}", "Instant", None, "Destroy target creature.");
    let small = table.card("{1}", "Creature", Some((3, 3)), "");
    let big = table.card("{7}", "Creature", Some((7, 7)), "");
    let walker = table.loyalty("{5}", "Planeswalker", 8, "");
    let land = table.card("", "Land", None, "{T}: Add {C}.");
    let mut game = Game::new(table);
    game.put(removal, P1, Zone::Hand);
    let small = game.put(small, P0, Zone::Battlefield);
    let big = game.put(big, P0, Zone::Battlefield);
    let walker = game.put(walker, P1, Zone::Battlefield);
    game.engine
        .state
        .objects
        .get_mut(&walker)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::Loyalty, 8);
    game.put(land, P1, Zone::Battlefield);
    game.engine.state.players.get_mut(&P1).unwrap().life = 3;
    game.main();
    let choice = game.pending.take().unwrap();
    game.engine
        .answer(&game.table, choice.id, Answer::Pass)
        .unwrap();
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    game.drive(
        |game, choice| {
            if matches!(choice.kind, ChoiceKind::DeclareAttackers { .. }) {
                Some(Answer::Attackers(vec![
                    (small, Target::Player(P1)),
                    (big, Target::Object(walker)),
                ]))
            } else if choice.who == P1 {
                Some(bot.decide(choice, &game.engine.view_for(P1)))
            } else {
                Some(Answer::Pass)
            }
        },
        |game| game.engine.state.step == Step::PostcombatMain && game.stack().is_empty(),
    );
    assert!(!game.engine.state.player(P1).has_lost);
    assert_eq!(game.engine.state.player(P1).life, 3);
    assert!(!game.engine.state.objects.contains_key(&small));
    assert!(game.engine.state.objects.contains_key(&big));
}

#[test]
fn threat_answers_a_draw_engine_before_waiting_for_its_own_turn() {
    let mut table = Table::default();
    let engine = table.card(
        "{1}",
        "Creature",
        Some((1, 1)),
        "At the beginning of your upkeep, draw a card.",
    );
    let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
    let mut game = Game::new(table);
    let engine = game.put(engine, P0, Zone::Battlefield);
    let removal = game.put(removal, P1, Zone::Hand);
    game.main();
    let choice = game.pending.take().unwrap();
    game.engine
        .answer(&game.table, choice.id, Answer::Pass)
        .unwrap();
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    let mut responded = false;
    game.drive(
        |game, choice| {
            if choice.who == P1 {
                let answer = bot.decide(choice, &game.engine.view_for(P1));
                if let Answer::Action(Action::Cast { object }) = &answer {
                    assert_eq!(*object, removal);
                    responded = true;
                }
                Some(answer)
            } else {
                Some(Answer::Pass)
            }
        },
        |game| !game.engine.state.objects.contains_key(&engine) && game.stack().is_empty(),
    );
    assert!(responded);
    assert_eq!(game.engine.state.active_player, P0);
    assert_eq!(game.count(Zone::Graveyard, P0), 1);
}

#[test]
fn threat_uses_reusable_removal_before_spending_a_card_on_a_lethal_attacker() {
    let mut table = Table::default();
    let attacker = table.card("{1}", "Creature", Some((3, 3)), "");
    let killer = table.card(
        "{1}",
        "Creature",
        Some((1, 1)),
        "{T}: Destroy target creature.",
    );
    let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
    let mut game = Game::new(table);
    let attacker = game.put(attacker, P0, Zone::Battlefield);
    let killer = game.put(killer, P1, Zone::Battlefield);
    let removal = game.put(removal, P1, Zone::Hand);
    game.engine.state.players.get_mut(&P1).unwrap().life = 3;
    game.main();
    let choice = game.pending.take().unwrap();
    game.engine
        .answer(&game.table, choice.id, Answer::Pass)
        .unwrap();
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    let mut activated = false;
    game.drive(|game, choice| {
        if matches!(choice.kind, ChoiceKind::DeclareAttackers { .. }) {
            Some(Answer::Attackers(vec![(attacker, Target::Player(P1))]))
        } else if choice.who == P1 {
            let answer = bot.decide(choice, &game.engine.view_for(P1));
            if let Answer::Action(action) = &answer {
                assert!(matches!(action, Action::ActivateAbility { source, .. } if *source == killer), "{action:?}");
                activated = true;
            }
            Some(answer)
        } else { Some(Answer::Pass) }
    }, |game| game.engine.state.step == Step::PostcombatMain && game.stack().is_empty());
    assert!(activated);
    assert_eq!(game.engine.state.player(P1).life, 3);
    assert!(game.engine.state.objects[&killer].tapped);
    assert_eq!(game.engine.state.objects[&removal].zone.zone, Zone::Hand);
    assert!(!game.engine.state.objects.contains_key(&attacker));
}

#[test]
fn threat_prioritizes_effective_engine_removal_over_an_expensive_new_creature() {
    let mut table = Table::default();
    let engine = table.card(
        "{1}",
        "Creature",
        Some((1, 1)),
        "Indestructible\nAt the beginning of your upkeep, draw a card.",
    );
    let destroy = table.card("{0}", "Instant", None, "Destroy target creature.");
    let exile = table.card("{1}", "Instant", None, "Exile target creature.");
    let bomb = table.card("{7}", "Creature", Some((7, 7)), "");
    let mut game = Game::new(table);
    let engine = game.put(engine, P1, Zone::Battlefield);
    game.put(destroy, P0, Zone::Hand);
    let exile = game.put(exile, P0, Zone::Hand);
    let bomb = game.put(bomb, P0, Zone::Hand);
    game.lands(7);
    game.main();
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    let choice = game.pending.take().unwrap();
    let ChoiceKind::Priority { legal } = &choice.kind else {
        panic!("priority")
    };
    assert!(legal.actions.contains(&Action::Cast { object: bomb }));
    let answer = bot.decide(&choice, &game.engine.view_for(P0));
    assert!(
        matches!(&answer, Answer::Action(Action::Cast { object }) if *object == exile),
        "{answer:?}"
    );
    game.engine.answer(&game.table, choice.id, answer).unwrap();
    game.drive(
        |game, choice| {
            Some(if matches!(choice.kind, ChoiceKind::Priority { .. }) {
                Answer::Pass
            } else {
                bot.decide(choice, &game.engine.view_for(choice.who))
            })
        },
        |game| !game.engine.state.objects.contains_key(&engine) && game.stack().is_empty(),
    );
    assert_eq!(game.engine.state.objects[&bomb].zone.zone, Zone::Hand);
    assert!(
        game.engine
            .state
            .objects
            .values()
            .any(|o| o.owner == P1 && o.zone.zone == Zone::Exile)
    );
}

#[test]
fn threat_indestructible_blocker_survives_a_bot_selected_deathtouch_block() {
    let mut table = Table::default();
    let attacker = table.card("{2}", "Creature", Some((2, 2)), "Deathtouch");
    let defender = table.card("{2}", "Creature", Some((2, 2)), "Indestructible");
    let mut game = Game::new(table);
    let attacker = game.put(attacker, P0, Zone::Battlefield);
    let defender = game.put(defender, P1, Zone::Battlefield);
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    game.main();
    let choice = game.pending.take().unwrap();
    game.engine
        .answer(&game.table, choice.id, Answer::Pass)
        .unwrap();
    let mut blocked = false;
    game.drive(
        |game, choice| Some(match &choice.kind {
            ChoiceKind::DeclareAttackers { .. } => Answer::Attackers(vec![(attacker, Target::Player(P1))]),
            ChoiceKind::DeclareBlockers { .. } => {
                let answer = bot.decide(choice, &game.engine.view_for(P1));
                assert!(matches!(&answer, Answer::Blocks(pairs) if pairs == &vec![(defender, attacker)]), "{answer:?}");
                blocked = true;
                answer
            }
            ChoiceKind::Priority { .. } => Answer::Pass,
            _ => bot.decide(choice, &game.engine.view_for(choice.who)),
        }),
        |game| game.engine.state.step == Step::PostcombatMain,
    );
    assert!(blocked);
    assert_eq!(game.engine.state.player(P1).life, 20);
    assert_eq!(
        game.engine.state.objects[&defender].zone.zone,
        Zone::Battlefield
    );
    assert!(!game.engine.state.objects.contains_key(&attacker));
}

#[test]
fn threat_deathtouch_ping_answers_a_large_engine_but_respects_indestructible() {
    use mtg_core::AbilityId;
    for (deathtouch, indestructible) in [(true, false), (false, false), (true, true)] {
        let mut table = Table::default();
        let pinger = table.card(
            "{1}",
            "Creature",
            Some((1, 1)),
            if deathtouch {
                "Deathtouch\n{T}: ~ deals 1 damage to target creature."
            } else {
                "{T}: ~ deals 1 damage to target creature."
            },
        );
        let engine = table.card(
            "{5}",
            "Creature",
            Some((5, 5)),
            if indestructible {
                "Indestructible\nAt the beginning of your upkeep, draw a card."
            } else {
                "At the beginning of your upkeep, draw a card."
            },
        );
        let mut game = Game::new(table);
        let pinger = game.put(pinger, P0, Zone::Battlefield);
        let engine = game.put(engine, P1, Zone::Battlefield);
        let bot = Bot::new(&game.table, (0..32).map(CardId));
        game.main();
        let choice = game.pending.take().unwrap();
        let answer = bot.decide(&choice, &game.engine.view_for(P0));
        if !deathtouch || indestructible {
            assert!(matches!(answer, Answer::Pass), "{answer:?}");
            continue;
        }
        // Deathtouch is the first ability; the activated damage is the second.
        assert!(
            matches!(answer, Answer::Action(Action::ActivateAbility { source, ability: AbilityId(1) }) if source == pinger),
            "{answer:?}"
        );
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
        assert!(!game.engine.state.objects.contains_key(&engine));
        assert!(game.engine.state.objects[&pinger].tapped);
    }
}

#[test]
fn threat_infect_blocks_prevent_poison_death_without_confusing_life_damage() {
    for (life, poison, infect_power, plain_power, block_infect) in [
        (20, 8, 2, 5, Some(true)),
        (20, 7, 2, 5, None),
        (2, 0, 8, 2, Some(false)),
    ] {
        let mut table = Table::default();
        let infected = table.card(
            "{2}",
            "Creature",
            Some((infect_power, infect_power)),
            "Infect",
        );
        let ordinary = table.card("{2}", "Creature", Some((plain_power, plain_power)), "");
        let defender = table.card("{1}", "Creature", Some((1, 1)), "");
        let mut game = Game::new(table);
        let infected = game.put(infected, P0, Zone::Battlefield);
        let ordinary = game.put(ordinary, P0, Zone::Battlefield);
        let defender = game.put(defender, P1, Zone::Battlefield);
        game.engine.state.players.get_mut(&P1).unwrap().life = life;
        game.engine.state.players.get_mut(&P1).unwrap().poison = poison;
        let bot = Bot::new(&game.table, (0..32).map(CardId));
        game.main();
        let choice = game.pending.take().unwrap();
        game.engine
            .answer(&game.table, choice.id, Answer::Pass)
            .unwrap();
        let mut selected = false;
        game.drive(
            |game, choice| {
                Some(match &choice.kind {
                    ChoiceKind::DeclareAttackers { .. } => Answer::Attackers(vec![
                        (infected, Target::Player(P1)),
                        (ordinary, Target::Player(P1)),
                    ]),
                    ChoiceKind::DeclareBlockers { .. } => {
                        let answer = bot.decide(choice, &game.engine.view_for(P1));
                        let expected = block_infect
                            .map(|infect| {
                                vec![(defender, if infect { infected } else { ordinary })]
                            })
                            .unwrap_or_default();
                        assert!(
                            matches!(&answer, Answer::Blocks(blocks) if *blocks == expected),
                            "{answer:?}"
                        );
                        selected = true;
                        answer
                    }
                    ChoiceKind::Priority { .. } => Answer::Pass,
                    _ => bot.decide(choice, &game.engine.view_for(choice.who)),
                })
            },
            |game| game.engine.state.step == Step::PostcombatMain,
        );
        assert!(selected);
        let player = game.engine.state.player(P1);
        assert!(!player.has_lost);
        assert_eq!(
            player.poison,
            poison
                + if block_infect == Some(true) {
                    0
                } else {
                    infect_power as u32
                }
        );
        assert_eq!(
            player.life,
            life - if block_infect == Some(false) {
                0
            } else {
                plain_power
            }
        );
    }
}

#[test]
fn threat_toxic_blocks_account_for_poison_life_and_each_damage_hit() {
    for (text, power, life, poison, should_block, poison_after, life_after) in [
        ("Toxic 3", 2, 20, 7, true, 7, 20),
        ("Toxic 3", 2, 20, 6, false, 9, 18),
        ("Toxic 3", 2, 2, 0, true, 0, 2),
        ("Infect\nToxic 2", 2, 20, 6, true, 6, 20),
        ("Double strike\nToxic 2", 1, 20, 6, true, 6, 20),
        ("Toxic 1\nToxic 2", 2, 20, 7, true, 7, 20),
        ("Toxic 9", 0, 20, 9, false, 9, 20),
    ] {
        let mut table = Table::default();
        let attacker = table.card("{2}", "Creature", Some((power, 2)), text);
        let defender = table.card("{1}", "Creature", Some((1, 1)), "");
        let mut game = Game::new(table);
        let attacker = game.put(attacker, P0, Zone::Battlefield);
        let defender = game.put(defender, P1, Zone::Battlefield);
        game.engine.state.players.get_mut(&P1).unwrap().life = life;
        game.engine.state.players.get_mut(&P1).unwrap().poison = poison;
        let bot = Bot::new(&game.table, (0..32).map(CardId));
        game.main();
        let choice = game.pending.take().unwrap();
        game.engine
            .answer(&game.table, choice.id, Answer::Pass)
            .unwrap();
        let mut selected = false;
        game.drive(
            |game, choice| {
                Some(match &choice.kind {
                    ChoiceKind::DeclareAttackers { .. } => {
                        Answer::Attackers(vec![(attacker, Target::Player(P1))])
                    }
                    ChoiceKind::DeclareBlockers { .. } => {
                        let answer = bot.decide(choice, &game.engine.view_for(P1));
                        let expected = if should_block {
                            vec![(defender, attacker)]
                        } else {
                            vec![]
                        };
                        assert!(
                            matches!(&answer, Answer::Blocks(blocks) if *blocks == expected),
                            "{text}: {answer:?}"
                        );
                        selected = true;
                        answer
                    }
                    ChoiceKind::Priority { .. } => Answer::Pass,
                    _ => bot.decide(choice, &game.engine.view_for(choice.who)),
                })
            },
            |game| game.engine.state.step == Step::PostcombatMain,
        );
        assert!(selected);
        let player = game.engine.state.player(P1);
        assert!(!player.has_lost, "{text}");
        assert_eq!(player.poison, poison_after, "{text}");
        assert_eq!(player.life, life_after, "{text}");
    }
}

#[test]
fn threat_removal_prioritizes_the_attacker_that_can_deliver_lethal_poison() {
    for (text, power, life, poison, remove_poison) in [
        ("Infect", 2, 20, 8, true),
        ("Toxic 3", 2, 20, 7, true),
        ("Double strike\nToxic 2", 1, 20, 6, true),
        ("Infect", 2, 20, 7, false),
        ("Infect", 8, 5, 0, false),
        ("Toxic 9", 0, 20, 9, false),
    ] {
        let mut table = Table::default();
        let poisoned = table.card("{1}", "Creature", Some((power, power.max(1))), text);
        let ordinary = table.card("{5}", "Creature", Some((5, 5)), "");
        let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
        let mut game = Game::new(table);
        let poisoned = game.put(poisoned, P0, Zone::Battlefield);
        let ordinary = game.put(ordinary, P0, Zone::Battlefield);
        game.put(removal, P1, Zone::Hand);
        game.engine.state.players.get_mut(&P1).unwrap().life = life;
        game.engine.state.players.get_mut(&P1).unwrap().poison = poison;
        let bot = Bot::new(&game.table, (0..32).map(CardId));
        game.main();
        let choice = game.pending.take().unwrap();
        game.engine
            .answer(&game.table, choice.id, Answer::Pass)
            .unwrap();
        let expected = if remove_poison { poisoned } else { ordinary };
        let mut targeted = false;
        game.drive(
            |game, choice| {
                if matches!(choice.kind, ChoiceKind::DeclareAttackers { .. }) {
                    return Some(Answer::Attackers(vec![
                        (poisoned, Target::Player(P1)),
                        (ordinary, Target::Player(P1)),
                    ]));
                }
                if choice.who != P1 {
                    return Some(Answer::Pass);
                }
                let answer = bot.decide(choice, &game.engine.view_for(P1));
                if let Answer::Targets(picks) = &answer {
                    assert_eq!(
                        picks,
                        &vec![vec![Target::Object(expected)]],
                        "{text} life={life} poison={poison}"
                    );
                    targeted = true;
                }
                Some(answer)
            },
            |game| game.engine.state.step == Step::PostcombatMain && game.stack().is_empty(),
        );
        assert!(targeted);
        assert!(!game.engine.state.player(P1).has_lost);
        assert!(!game.engine.state.objects.contains_key(&expected));
        assert_eq!(
            game.engine.state.objects[&if remove_poison { ordinary } else { poisoned }]
                .zone
                .zone,
            Zone::Battlefield
        );
    }
}
