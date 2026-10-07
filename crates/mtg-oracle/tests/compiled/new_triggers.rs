//! "Whenever one or more +1/+1 counters are put on …", "whenever one or more creature cards
//! leave your graveyard": once for each placement or batch.

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

#[test]
fn counters_put_on_it_trigger_once_per_placement() {
    let mut table = Table::default();
    let scholar = table.card(
        "{1}{U}",
        "Creature — Bear",
        Some((1, 1)),
        "Whenever one or more +1/+1 counters are put on this creature, draw a card.",
    );
    let feed = table.card(
        "{G}",
        "Instant",
        None,
        "Put two +1/+1 counters on target creature.",
    );
    let mut game = Game::new(table);
    game.lands(1);
    let scholar = game.put(scholar, P0, Zone::Battlefield);
    let feed = game.put(feed, P0, Zone::Hand);
    game.main();
    let hand = game.count(Zone::Hand, P0);
    game.act(
        Action::Cast { object: feed },
        &[Target::Object(scholar)],
        &[],
    );
    assert_eq!(
        game.count(Zone::Hand, P0),
        hand,
        "cast one, drew one — not two"
    );
}

#[test]
fn creature_cards_leaving_the_graveyard_together_trigger_once() {
    let mut table = Table::default();
    let ghoul = table.card(
        "{1}{B}",
        "Creature — Bear",
        Some((2, 2)),
        "Whenever one or more creature cards leave your graveyard, you gain 1 life.",
    );
    let raise = table.card(
        "{1}{B}",
        "Sorcery",
        None,
        "Return up to two target creature cards from your graveyard to your hand.",
    );
    let exile = table.card(
        "{B}",
        "Instant",
        None,
        "Exile target card from a graveyard.",
    );
    let bear = table.bear();
    let rite = table.card("{B}", "Sorcery", None, "You gain 1 life.");
    let mut game = Game::new(table);
    game.lands(3);
    game.put(ghoul, P0, Zone::Battlefield);
    let a = game.put(bear, P0, Zone::Graveyard);
    let b = game.put(bear, P0, Zone::Graveyard);
    let sorcery = game.put(rite, P0, Zone::Graveyard);
    let raise = game.put(raise, P0, Zone::Hand);
    let exile = game.put(exile, P0, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: raise },
        &[Target::Object(a), Target::Object(b)],
        &[],
    );
    assert_eq!(game.life(P0), 21, "two at once: once");
    game.act(
        Action::Cast { object: exile },
        &[Target::Object(sorcery)],
        &[],
    );
    assert_eq!(game.life(P0), 21, "a sorcery card isn't a creature card");
}

#[test]
fn a_creature_attacking_you_triggers_but_your_own_attack_does_not() {
    use mtg_engine::{
        Progress,
        choice::{Answer, ChoiceKind},
    };
    let mut table = Table::default();
    let ward = table.card(
        "{1}{W}",
        "Enchantment",
        None,
        "Whenever a creature attacks you or a planeswalker you control, you gain 1 life.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.put(ward, P0, Zone::Battlefield);
    let mine = game.put(bear, P0, Zone::Battlefield);
    let theirs = game.put(bear, P1, Zone::Battlefield);
    game.main();
    game.combat(&[mine], &[], &[], &[]);
    assert_eq!(game.life(P0), 20, "my own attack isn't against me");
    // P1's turn: P1 attacks with its bear, P0 doesn't block.
    let c = game.pending.take().unwrap();
    game.engine.answer(&game.table, c.id, Answer::Pass).unwrap();
    for _ in 0..10_000 {
        let Progress::NeedsChoice(c) = game.engine.advance(&game.table) else {
            continue;
        };
        if game.engine.state.active_player == P1
            && game.engine.state.step == mtg_core::Step::PostcombatMain
        {
            break;
        }
        let answer = match &c.kind {
            ChoiceKind::DeclareAttackers { .. } => Answer::Objects(vec![theirs]),
            ChoiceKind::DeclareBlockers { .. } => Answer::Blocks(vec![]),
            ChoiceKind::Priority { .. } => Answer::Pass,
            _ => c.default.clone().unwrap_or(Answer::Pass),
        };
        game.engine.answer(&game.table, c.id, answer).unwrap();
    }
    assert_eq!(
        game.life(P0),
        20 + 1 - 2,
        "gained 1 as it attacked, then took 2"
    );
}

#[test]
fn combat_damage_to_a_player_or_planeswalker_and_damage_to_a_creature() {
    let mut table = Table::default();
    let raider = table.card(
        "{1}{R}",
        "Creature — Bear",
        Some((2, 2)),
        "Whenever this creature deals combat damage to a player or planeswalker, draw a card.",
    );
    let brute = table.card(
        "{1}{G}",
        "Creature — Bear",
        Some((3, 3)),
        "Whenever this creature deals damage to a creature, you gain 2 life.",
    );
    let wall = table.card("{1}{W}", "Creature — Wall", Some((0, 5)), "");
    let mut game = Game::new(table);
    let raider = game.put(raider, P0, Zone::Battlefield);
    let brute = game.put(brute, P0, Zone::Battlefield);
    let wall = game.put(wall, P1, Zone::Battlefield);
    game.main();
    let hand = game.count(Zone::Hand, P0);
    game.combat(&[raider, brute], &[(wall, brute)], &[], &[]);
    assert_eq!(game.count(Zone::Hand, P0), hand + 1, "the raider hit P1");
    assert_eq!(game.life(P0), 22, "the brute hit the wall");
}

#[test]
fn discarding_several_at_once_triggers_once_and_scry_or_surveil_both_count() {
    let mut table = Table::default();
    let madman = table.card(
        "{1}{R}",
        "Enchantment",
        None,
        "Whenever you discard one or more cards, you gain 1 life.\nWhenever you scry or \
         surveil, you gain 10 life.",
    );
    let wheel = table.card("{R}", "Sorcery", None, "Discard two cards.");
    let scry = table.card("{U}", "Sorcery", None, "Scry 1.");
    let surveil = table.card("{B}", "Sorcery", None, "Surveil 1.");
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(3);
    game.put(madman, P0, Zone::Battlefield);
    let wheel = game.put(wheel, P0, Zone::Hand);
    let scry = game.put(scry, P0, Zone::Hand);
    let surveil = game.put(surveil, P0, Zone::Hand);
    let a = game.put(bear, P0, Zone::Hand);
    let b = game.put(bear, P0, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: wheel },
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![a, b])],
    );
    assert_eq!(game.life(P0), 21, "two cards at once: once");
    game.act(Action::Cast { object: scry }, &[], &[]);
    assert_eq!(game.life(P0), 31);
    game.act(Action::Cast { object: surveil }, &[], &[]);
    assert_eq!(game.life(P0), 41);
}

#[test]
fn a_source_dealing_damage_to_it_and_first_spell_each_turn() {
    let mut table = Table::default();
    let spirit = table.card(
        "{2}{R}",
        "Creature — Bear",
        Some((2, 4)),
        "Whenever a source deals damage to this creature, you gain 3 life.\nWhenever you cast \
         your first spell each turn, you gain 10 life.",
    );
    let shock = table.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
    let mut game = Game::new(table);
    game.lands(2);
    let spirit = game.put(spirit, P0, Zone::Battlefield);
    let first = game.put(shock, P0, Zone::Hand);
    let second = game.put(shock, P0, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: first },
        &[Target::Object(spirit)],
        &[],
    );
    assert_eq!(game.life(P0), 33, "first spell: 10, damage to it: 3");
    game.act(
        Action::Cast { object: second },
        &[Target::Object(spirit)],
        &[],
    );
    assert_eq!(game.life(P0), 36, "not the first spell any more");
}

#[test]
fn casting_a_kicked_spell_triggers_but_an_unkicked_one_does_not() {
    let mut table = Table::default();
    let mage = table.card(
        "{1}{U}",
        "Creature — Bear",
        Some((1, 1)),
        "Whenever you cast a kicked spell, you gain 5 life.",
    );
    let bolt = table.card(
        "{R}",
        "Instant",
        None,
        "Kicker {1}\n~ deals 1 damage to any target.",
    );
    let mut game = Game::new(table);
    game.lands(3);
    game.put(mage, P0, Zone::Battlefield);
    let first = game.put(bolt, P0, Zone::Hand);
    let second = game.put(bolt, P0, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: first },
        &[Target::Player(P1)],
        &[mtg_engine::choice::Answer::Bool(false)],
    );
    assert_eq!(game.life(P0), 20, "not kicked");
    game.act(
        Action::Cast { object: second },
        &[Target::Player(P1)],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    assert_eq!(game.life(P0), 25, "kicked");
}

#[test]
fn searching_library_and_graveyard_for_a_named_card() {
    use mtg_engine::choice::Answer;
    for from_graveyard in [true, false] {
        let mut table = Table::default();
        let finder = table.card(
            "{1}{W}",
            "Creature — Bear",
            Some((2, 2)),
            "When this creature enters, you may search your library and/or graveyard for a \
             card named Ajani, Inspiring Leader, reveal it, and put it into your hand. If you \
             search your library this way, shuffle.",
        );
        let ajani = table.named_card(
            "Ajani, Inspiring Leader",
            "{4}{W}{W}",
            "Legendary Enchantment",
            None,
            "",
        );
        let mut game = Game::new(table);
        game.lands(2);
        let finder = game.put(finder, P0, Zone::Hand);
        let zone = if from_graveyard {
            Zone::Graveyard
        } else {
            Zone::Library
        };
        let ajani = game.put(ajani, P0, zone);
        game.main();
        let hand = game.count(Zone::Hand, P0);
        game.act(
            Action::Cast { object: finder },
            &[],
            &[Answer::Objects(vec![ajani])],
        );
        assert_eq!(game.count(Zone::Hand, P0), hand, "cast one, found one");
        let shuffled = game
            .engine
            .log
            .iter()
            .any(|e| e.event == mtg_core::Event::Shuffled { player: P0 });
        assert_eq!(shuffled, !from_graveyard);
    }
}
