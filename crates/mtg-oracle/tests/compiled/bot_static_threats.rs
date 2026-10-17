//! Static locks, graveyard resources and replacement wins, through real choices.

use super::harness::*;
use mtg_core::{AbilityId, CardId, ObjectId, Zone, ZoneRef};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, Choice, ChoiceKind},
};
use mtg_policy::bot::Bot;

fn give_priority_to_p1(game: &mut Game) -> Choice {
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
    panic!("no priority window");
}

fn response(game: &mut Game, choice: Choice, expected: Option<ObjectId>) {
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    let answer = bot.decide(&choice, &game.engine.view_for(P1));
    match expected {
        Some(id) => assert!(
            matches!(&answer, Answer::Action(Action::Cast { object }) if *object == id),
            "{answer:?}"
        ),
        None => {
            assert!(matches!(answer, Answer::Pass), "{answer:?}");
            return;
        }
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

fn empty_library(game: &mut Game, player: mtg_core::PlayerId) {
    let zone = ZoneRef::of(Zone::Library, player);
    for id in game.engine.state.objects_in(zone) {
        game.engine.state.objects.remove(&id);
    }
    game.engine.state.zone_order.insert(zone, Vec::new());
    game.engine.state.bump();
}

#[test]
fn static_threat_removes_a_casting_lock_when_it_blocks_our_visible_commander() {
    for (owner, commander) in [(P0, true), (P0, false), (P1, true)] {
        let mut table = Table::default();
        let lock = table.card(
            "{1}{W}",
            "Creature",
            Some((1, 3)),
            "Your opponents can't cast spells from anywhere other than their hands.",
        );
        let general = table.card("{0}", "Legendary Creature", Some((2, 2)), "");
        let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
        let mut game = Game::new(table);
        let lock = game.put(lock, owner, Zone::Battlefield);
        let general = if commander {
            game.engine.state.commander.commanders.insert(P1, general);
            Some(game.put(general, P1, Zone::Command))
        } else {
            None
        };
        let removal = game.put(removal, P1, Zone::Hand);
        game.main();
        let choice = give_priority_to_p1(&mut game);
        let bot = Bot::new(&game.table, (0..32).map(CardId));
        let urgent = owner == P0 && commander;
        assert_eq!(
            bot.strategic_threat_score(&game.engine.view_for(P1), lock) >= 32,
            urgent
        );
        response(&mut game, choice, urgent.then_some(removal));
        if urgent {
            assert!(!game.engine.state.objects.contains_key(&lock));
            let actions = game.until(P1, mtg_core::Step::PrecombatMain);
            assert!(actions.contains(&Action::Cast {
                object: general.unwrap()
            }));
        } else {
            assert_eq!(game.engine.state.objects[&removal].zone.zone, Zone::Hand);
        }
    }
}

#[test]
fn static_threat_escape_requires_a_matching_card_and_separate_graveyard_fuel() {
    for (fuel, owner, nonland) in [(2, P0, true), (3, P0, true), (3, P1, true), (3, P0, false)] {
        let mut table = Table::default();
        let breach = table.card("{1}{R}", "Enchantment", None, "Each nonland card in your graveyard has escape. The escape cost is equal to the card's mana cost plus exile three other cards from your graveyard.");
        let card = if nonland {
            table.card("{0}", "Sorcery", None, "Draw a card.")
        } else {
            table.card("", "Land", None, "")
        };
        let filler = if nonland { table.bear() } else { card };
        let removal = table.card("{0}", "Instant", None, "Destroy target enchantment.");
        let mut game = Game::new(table);
        let breach = game.put(breach, P0, Zone::Battlefield);
        game.put(card, owner, Zone::Graveyard);
        for _ in 0..fuel {
            game.put(filler, owner, Zone::Graveyard);
        }
        let removal = game.put(removal, P1, Zone::Hand);
        game.main();
        let choice = give_priority_to_p1(&mut game);
        let urgent = fuel == 3 && owner == P0 && nonland;
        let bot = Bot::new(&game.table, (0..32).map(CardId));
        assert_eq!(
            bot.strategic_threat_score(&game.engine.view_for(P1), breach) >= 32,
            urgent
        );
        response(&mut game, choice, urgent.then_some(removal));
        if urgent {
            assert!(!game.engine.state.objects.contains_key(&breach));
        }
    }
}

#[test]
fn static_threat_casting_cap_counts_lands_instead_of_mana_rocks() {
    for lands in [0, 3] {
        let mut table = Table::default();
        let lock = table.card("{W}{U}", "Creature", Some((2, 2)), "Each opponent can't cast noncreature spells with mana value greater than the number of lands that player controls.");
        let spell = table.card("{3}", "Sorcery", None, "Draw three cards.");
        let rock = table.card("{0}", "Artifact", None, "{T}: Add {C}{C}{C}{C}.");
        let land = table.mountain();
        let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
        let mut game = Game::new(table);
        let lock = game.put(lock, P0, Zone::Battlefield);
        game.put(spell, P1, Zone::Hand);
        game.put(rock, P1, Zone::Battlefield);
        for _ in 0..lands {
            game.put(land, P1, Zone::Battlefield);
        }
        let removal = game.put(removal, P1, Zone::Hand);
        game.main();
        let choice = give_priority_to_p1(&mut game);
        let bot = Bot::new(&game.table, (0..32).map(CardId));
        assert_eq!(
            bot.strategic_threat_score(&game.engine.view_for(P1), lock) >= 32,
            lands == 0
        );
        response(&mut game, choice, (lands == 0).then_some(removal));
    }
}

#[test]
fn static_threat_search_interference_matters_when_we_have_a_visible_tutor() {
    for text in [
        "Spells and abilities your opponents control can't cause their controller to search their library.",
        "You control your opponents while they're searching their libraries.",
    ] {
        for tutor in 0..4 {
            let mut table = Table::default();
            let lock = table.card("{1}", "Enchantment", None, text);
            let search = match tutor {
                2 => table.card("{1}", "Creature", Some((1, 1)), "When ~ enters, search your library for a card, put it into your hand, then shuffle."),
                3 => table.card("{1}", "Creature", Some((1, 1)), "{T}: Search your library for a card, put it into your hand, then shuffle."),
                _ => table.card("{0}", "Sorcery", None, "Search your library for a card, put it into your hand, then shuffle."),
            };
            let removal = table.card("{0}", "Instant", None, "Destroy target enchantment.");
            let mut game = Game::new(table);
            let lock = game.put(lock, P0, Zone::Battlefield);
            if tutor != 0 {
                game.put(
                    search,
                    P1,
                    if tutor == 1 {
                        Zone::Hand
                    } else {
                        Zone::Battlefield
                    },
                );
            }
            let removal = game.put(removal, P1, Zone::Hand);
            game.main();
            let choice = give_priority_to_p1(&mut game);
            let urgent = tutor == 1 || tutor == 3;
            response(&mut game, choice, urgent.then_some(removal));
            if urgent {
                assert!(!game.engine.state.objects.contains_key(&lock));
            }
        }
    }
}

#[test]
fn static_threat_replacement_win_urgency_depends_on_the_opponents_library() {
    for empty in [false, true] {
        let mut table = Table::default();
        let maniac = table.card("{2}{U}", "Creature", Some((2, 2)), "If you would draw a card while your library has no cards in it, you win the game instead.");
        let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
        let mut game = Game::new(table);
        let maniac = game.put(maniac, P0, Zone::Battlefield);
        let removal = game.put(removal, P1, Zone::Hand);
        game.main();
        if empty {
            empty_library(&mut game, P0);
        }
        let choice = give_priority_to_p1(&mut game);
        response(&mut game, choice, empty.then_some(removal));
        if empty {
            assert!(!game.engine.state.objects.contains_key(&maniac));
        }
    }
}

#[test]
fn static_threat_removes_the_replacement_win_enabler_in_response_to_a_draw() {
    let mut table = Table::default();
    let maniac = table.card(
        "{2}{U}",
        "Creature",
        Some((2, 2)),
        "If you would draw a card while your library has no cards in it, you win the game instead.",
    );
    let draw = table.card("{0}", "Instant", None, "Draw a card.");
    let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
    let mut game = Game::new(table);
    let maniac = game.put(maniac, P0, Zone::Battlefield);
    let draw = game.put(draw, P0, Zone::Hand);
    let removal = game.put(removal, P1, Zone::Hand);
    game.main();
    empty_library(&mut game, P0);
    game.act_holding(Action::Cast { object: draw }, &[]);
    let choice = give_priority_to_p1(&mut game);
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    let answer = bot.decide(&choice, &game.engine.view_for(P1));
    assert!(
        matches!(&answer, Answer::Action(Action::Cast { object }) if *object == removal),
        "{answer:?}"
    );
    game.engine.answer(&game.table, choice.id, answer).unwrap();
    for _ in 0..300 {
        match game.engine.advance(&game.table) {
            Progress::Continue => {}
            Progress::GameOver { winners } => {
                assert_eq!(winners, vec![P1]);
                assert!(!game.engine.state.objects.contains_key(&maniac));
                return;
            }
            Progress::NeedsChoice(choice) => {
                let answer = if choice.who == P1 {
                    bot.decide(&choice, &game.engine.view_for(P1))
                } else {
                    Answer::Pass
                };
                game.engine.answer(&game.table, choice.id, answer).unwrap();
            }
        }
    }
    panic!("the empty-library draw did not end the game");
}

#[test]
fn static_threat_does_not_confuse_source_removal_with_countering_a_win_ability() {
    let mut table = Table::default();
    let source = table.card("{1}", "Creature", Some((1, 1)), "{T}: You win the game.");
    let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
    let mut game = Game::new(table);
    let source = game.put(source, P0, Zone::Battlefield);
    let removal = game.put(removal, P1, Zone::Hand);
    game.main();
    game.act_holding(
        Action::ActivateAbility {
            source,
            ability: AbilityId(0),
        },
        &[],
    );
    let choice = give_priority_to_p1(&mut game);
    response(&mut game, choice, None);
    assert_eq!(game.engine.state.objects[&removal].zone.zone, Zone::Hand);
}

#[test]
fn static_threat_uses_an_ability_when_the_lock_prevents_casting_removal() {
    let mut table = Table::default();
    let lock = table.card(
        "{1}",
        "Creature",
        Some((1, 3)),
        "Your opponents can't cast noncreature spells.",
    );
    let killer = table.card(
        "{1}",
        "Creature",
        Some((1, 1)),
        "{T}: Destroy target creature.",
    );
    let removal = table.card("{0}", "Instant", None, "Destroy target creature.");
    let mut game = Game::new(table);
    let lock = game.put(lock, P0, Zone::Battlefield);
    let killer = game.put(killer, P1, Zone::Battlefield);
    let removal = game.put(removal, P1, Zone::Hand);
    game.main();
    let choice = give_priority_to_p1(&mut game);
    let ChoiceKind::Priority { legal } = &choice.kind else {
        panic!("priority")
    };
    assert!(!legal.actions.contains(&Action::Cast { object: removal }));
    let bot = Bot::new(&game.table, (0..32).map(CardId));
    let answer = bot.decide(&choice, &game.engine.view_for(P1));
    assert!(
        matches!(&answer, Answer::Action(Action::ActivateAbility { source, .. }) if *source == killer),
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
        |game| !game.engine.state.objects.contains_key(&lock) && game.stack().is_empty(),
    );
    let actions = game.until(P1, mtg_core::Step::PrecombatMain);
    assert!(actions.contains(&Action::Cast { object: removal }));
}
