//! The bot with a blast, a counterspell or narrow removal in hand: it must neither loop on a
//! cast it cannot finish nor waste the card, and it should counter what it can.

use super::harness::*;
use mtg_core::{CardId, PlayerId, Zone};
use mtg_engine::{Progress, actions::Action, choice::Answer};
use mtg_policy::bot::Bot;

const PYROBLAST: &str =
    "Choose one —\n• Counter target spell if it's blue.\n• Destroy target permanent if it's blue.";

#[test]
fn search_declines_a_fatal_optional_draw_and_accepts_a_useful_one() {
    use mtg_core::ZoneRef;
    use mtg_engine::choice::ChoiceKind;
    use mtg_policy::search::{Budget, Searcher};

    for empty_library in [false, true] {
        let mut table = Table::default();
        let draw = table.card("{0}", "Sorcery", None, "You may draw a card.");
        let mut game = Game::new(table);
        let spell = game.put(draw, P0, Zone::Hand);
        game.main();
        let retained_hand = game
            .engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .len()
            - 1;
        if empty_library {
            let library = ZoneRef::of(Zone::Library, P0);
            for id in game.engine.state.objects_in(library) {
                game.engine.state.objects.remove(&id);
            }
            game.engine.state.zone_order.insert(library, Vec::new());
            game.engine.state.bump();
        }
        let choice = game.pending.take().unwrap();
        game.engine
            .answer(
                &game.table,
                choice.id,
                Answer::Action(Action::Cast { object: spell }),
            )
            .unwrap();
        let searcher = Searcher::new(
            Bot::new(&game.table, (0..2).map(CardId)),
            Budget {
                playouts: 8,
                horizon_turns: 0,
                max_choices: 40,
                ..Default::default()
            },
            23,
        );
        let mut compared = false;
        let mut resolved = false;
        for _ in 0..100 {
            match game.engine.advance(&game.table) {
                Progress::Continue => {}
                Progress::GameOver { .. } => break,
                Progress::NeedsChoice(choice) => {
                    if compared && game.stack().is_empty() {
                        resolved = true;
                        break;
                    }
                    let answer = if matches!(choice.kind, ChoiceKind::Confirm) {
                        compared = true;
                        let answer = searcher.decide(&game.engine, &game.table, &choice);
                        assert!(
                            matches!(answer, Answer::Bool(accept) if accept != empty_library),
                            "{answer:?}, empty={empty_library}"
                        );
                        answer
                    } else {
                        searcher
                            .bot
                            .decide(&choice, &game.engine.view_for(choice.who))
                    };
                    game.engine.answer(&game.table, choice.id, answer).unwrap();
                }
            }
        }
        assert!(compared && resolved);
        assert!(!game.engine.state.player(P0).has_lost);
        assert_eq!(
            game.engine
                .state
                .objects_in(ZoneRef::of(Zone::Hand, P0))
                .len(),
            retained_hand + usize::from(!empty_library)
        );
    }
}

/// Both seats played by the bot through turn `last`, as the app plays a match. Every
/// answer must be accepted, and the number of choices must stay sane — a cast that
/// rewinds and is offered again makes thousands.
fn play(g: &mut Game, cards: u32, last: u32) -> Vec<(PlayerId, Action)> {
    let bot = Bot::new(&g.table, (0..cards).map(CardId));
    let mut actions = Vec::new();
    let mut choices = 0;
    while g.engine.state.turn <= last {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
                choices += 1;
                assert!(choices < 2_000, "the game stalled: {actions:?}");
                let view = mtg_engine::view::project(&g.engine.state, c.who);
                let answer = bot.decide(&c, &view);
                if let Answer::Action(a) = &answer {
                    actions.push((c.who, a.clone()));
                }
                g.engine
                    .answer(&g.table, c.id, answer.clone())
                    .unwrap_or_else(|e| panic!("{:?} -> {answer:?}: {e:?}", c.kind));
            }
        }
    }
    actions
}

fn lands(g: &mut Game, who: PlayerId, n: usize) {
    let land = g
        .table
        .card("", "Land", None, "{T}: Add one mana of any color.");
    for _ in 0..n {
        g.put(land, who, Zone::Battlefield);
    }
}

fn casts(actions: &[(PlayerId, Action)], who: PlayerId, object: mtg_core::ObjectId) -> usize {
    actions
        .iter()
        .filter(|(p, a)| *p == who && a.play().is_some_and(|(o, _, _)| o == object))
        .count()
}

#[test]
fn a_blast_with_nothing_blue_to_hit_is_held() {
    let mut t = Table::default();
    let blast = t.card("{R}", "Instant", None, PYROBLAST);
    let green = t.card("{G}", "Creature", Some((2, 2)), "");
    let mut g = Game::new(t);
    lands(&mut g, P0, 3);
    let spell = g.put(blast, P0, Zone::Hand);
    g.put(green, P1, Zone::Battlefield);
    let actions = play(&mut g, 4, 5);
    assert_eq!(casts(&actions, P0, spell), 0, "{actions:?}");
    assert!(g.engine.state.objects.contains_key(&spell));
}

#[test]
fn a_blast_destroys_the_opponents_blue_permanent_not_its_own() {
    let mut t = Table::default();
    let blast = t.card("{R}", "Instant", None, PYROBLAST);
    let blue = t.card("{U}", "Creature", Some((2, 2)), "");
    let mut g = Game::new(t);
    lands(&mut g, P0, 3);
    g.put(blast, P0, Zone::Hand);
    let mine = g.put(blue, P0, Zone::Battlefield);
    let theirs = g.put(blue, P1, Zone::Battlefield);
    let actions = play(&mut g, 4, 2);
    assert!(g.engine.state.objects.contains_key(&mine), "{actions:?}");
    assert!(!g.engine.state.objects.contains_key(&theirs), "{actions:?}");
}

#[test]
fn a_blast_counters_the_opponents_blue_spell() {
    let mut t = Table::default();
    let blast = t.card("{R}", "Instant", None, PYROBLAST);
    let blue = t.card("{U}", "Creature", Some((2, 2)), "");
    let mut g = Game::new(t);
    lands(&mut g, P0, 1);
    lands(&mut g, P1, 1);
    let spell = g.put(blast, P0, Zone::Hand);
    let theirs = g.put(blue, P1, Zone::Hand);
    let actions = play(&mut g, 4, 3);
    assert_eq!(casts(&actions, P1, theirs), 1, "{actions:?}");
    assert_eq!(casts(&actions, P0, spell), 1, "{actions:?}");
    assert_eq!(g.count(Zone::Graveyard, P1), 1, "countered: {actions:?}");
}

#[test]
fn a_counterspell_waits_for_a_spell_and_the_game_goes_on() {
    let mut t = Table::default();
    let counter = t.card("{U}{U}", "Instant", None, "Counter target spell.");
    let bear = t.card("{G}", "Creature", Some((2, 2)), "");
    let mut g = Game::new(t);
    // Enough for the bear it draws and the counterspell both.
    lands(&mut g, P0, 4);
    lands(&mut g, P1, 1);
    let spell = g.put(counter, P0, Zone::Hand);
    let theirs = g.put(bear, P1, Zone::Hand);
    let actions = play(&mut g, 4, 3);
    assert_eq!(casts(&actions, P1, theirs), 1, "{actions:?}");
    assert_eq!(casts(&actions, P0, spell), 1, "{actions:?}");
    assert_eq!(g.count(Zone::Graveyard, P1), 1, "countered: {actions:?}");
}

#[test]
fn removal_without_a_legal_target_does_not_stall_the_bot() {
    let mut t = Table::default();
    let removal = t.card("{B}", "Instant", None, "Destroy target blue creature.");
    let green = t.card("{G}", "Creature", Some((2, 2)), "");
    let mut g = Game::new(t);
    lands(&mut g, P0, 2);
    g.put(removal, P0, Zone::Hand);
    g.put(green, P1, Zone::Battlefield);
    play(&mut g, 4, 5);
}

#[test]
fn a_mode_without_targets_is_refused_and_not_the_default() {
    let mut t = Table::default();
    let blast = t.card("{0}", "Instant", None, PYROBLAST);
    let mut g = Game::new(t);
    let spell = g.put(blast, P0, Zone::Hand);
    // A permanent to point the destroy mode at; with none, the blast isn't offered at all.
    lands(&mut g, P1, 1);
    let prompt = {
        g.main();
        g.pending.take().unwrap()
    };
    g.engine
        .answer(
            &g.table,
            prompt.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let modes = loop {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => break c,
            Progress::GameOver { .. } => panic!("game ended"),
        }
    };
    // Nothing on the stack to counter: only the destroy mode can be chosen (CR 700.2a).
    assert!(matches!(&modes.default, Some(Answer::Modes(m)) if m == &[1]));
    assert!(
        g.engine
            .answer(&g.table, modes.id, Answer::Modes(vec![0]))
            .is_err()
    );
}

/// Simian Spirit Guide can't pay for itself: once it is cast it is no longer in hand to exile.
/// Counted as its own mana, the cast was offered, accepted, and silently taken back — forever,
/// for a bot.
#[test]
fn a_spirit_guide_is_not_its_own_mana() {
    let mut t = Table::default();
    let guide = t.card(
        "{2}{R}",
        "Creature — Ape Spirit",
        Some((2, 2)),
        "Exile this card from your hand: Add {R}.",
    );
    let mut g = Game::new(t);
    lands(&mut g, P0, 2);
    let card = g.put(guide, P0, Zone::Hand);
    assert!(!g.main().contains(&Action::Cast { object: card }));
}

#[test]
fn a_bot_with_a_spirit_guide_and_two_lands_keeps_playing() {
    let mut t = Table::default();
    let guide = t.card(
        "{2}{R}",
        "Creature — Ape Spirit",
        Some((2, 2)),
        "Exile this card from your hand: Add {R}.",
    );
    let mut g = Game::new(t);
    lands(&mut g, P0, 2);
    g.put(guide, P0, Zone::Hand);
    play(&mut g, 2, 4);
}
