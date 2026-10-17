//! Find actions the engine accepts and then silently takes back.
//!
//! Plays the saved decks against each other, bot against bot, and reports every action
//! that was accepted but left the game exactly as it was: the same player back at priority,
//! the same stack, and the log rewound to where it stood. To a player that is an ability
//! that "just didn't happen"; to a bot it is a loop.
//!
//! Run: cargo run -p mtg-app --example rewind_sweep -- [games per pairing] [turns]

use std::collections::BTreeMap;

use mtg_app::decks::{LocalSource, deck_list, deck_spec, open_store};
use mtg_core::PlayerId;
use mtg_engine::{Engine, Progress, actions::Action, choice::Answer};
use mtg_ir::PrintedCards;
use mtg_net::fairness::Seed;
use mtg_policy::bot::Bot;
use mtg_session::game::{Format, GameCards, new_game};

fn main() {
    // A step that never returns is reported by a watchdog: what was resolving, and since when.
    let watched: &'static std::sync::Mutex<(std::time::Instant, String)> = Box::leak(Box::new(
        std::sync::Mutex::new((std::time::Instant::now(), String::new())),
    ));
    std::thread::spawn(move || {
        let mut reported = None;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let (since, what) = watched.lock().unwrap().clone();
            if since.elapsed().as_secs() >= 10 && reported != Some(since) {
                println!("STUCK for {}s resolving: {what}", since.elapsed().as_secs());
                reported = Some(since);
            }
        }
    });
    let args: Vec<String> = std::env::args().skip(1).collect();
    let games: u8 = args.first().and_then(|a| a.parse().ok()).unwrap_or(2);
    let turns: u32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(15);

    let store = open_store();
    let decks: Vec<_> = deck_list(store.as_ref())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(id, name)| deck_spec(store.as_ref(), id).ok().map(|d| (name, d)))
        .collect();
    let source = LocalSource::standard();
    // Card name → (times silently undone, an example).
    let mut found: BTreeMap<String, (u32, String)> = BTreeMap::new();
    let mut played = 0;

    // Cards the compiler can't play yet are left out, so every deck gets games.
    let decks: Vec<_> = decks
        .into_iter()
        .map(|(name, mut deck)| {
            deck.main
                .retain(|(key, _)| mtg_session::game::CardSource::card(&source, key).is_some());
            deck.side.clear();
            (name, deck)
        })
        .collect();
    let pairs = (0..decks.len()).flat_map(|i| (i + 1..decks.len()).map(move |j| (i, j)));
    for (i, j) in pairs {
        let ((a_name, a), (b_name, b)) = (&decks[i], &decks[j]);
        let commander = a.commander.is_some() && b.commander.is_some();
        if a.commander.is_some() != b.commander.is_some() {
            continue;
        }
        let format = if commander {
            Format::Commander
        } else {
            Format::Constructed
        };
        let cards = match GameCards::build([a, b], &source) {
            Ok(cards) => cards,
            Err(missing) => {
                println!(
                    "skip {a_name} vs {b_name}: {} cards unplayable",
                    missing.len()
                );
                continue;
            }
        };
        let bot = Bot::new(&cards, cards.ids());
        for game in 0..games {
            let seed = Seed([(i * 7 + j) as u8 ^ game.wrapping_mul(37); 32]);
            let state = new_game(format, &cards, [a, b], &seed, game, PlayerId(game % 2));
            let mut engine = Engine::new(state);
            // SWEEP_ONLY="curious vs millbert:4" replays a single game.
            if let Ok(only) = std::env::var("SWEEP_ONLY")
                && only != format!("{a_name} vs {b_name}:{game}")
            {
                continue;
            }
            played += 1;
            let started = std::time::Instant::now();
            let mut choices = 0u32;
            let mut quiet = 0u32;
            // After an accepted action: who took it, the stack and log as they were.
            let mut pending: Option<(PlayerId, usize, usize, Action)> = None;
            for _ in 0..200_000 {
                {
                    let top = engine
                        .view_for(PlayerId(0))
                        .stack
                        .last()
                        .and_then(|o| engine.state.objects.get(o))
                        .and_then(|o| cards.face(o.card, o.face))
                        .map_or_else(String::new, |f| f.name.to_string());
                    *watched.lock().unwrap() = (
                        std::time::Instant::now(),
                        format!(
                            "{a_name} vs {b_name} game {game}, turn {}: {top}",
                            engine.state.turn
                        ),
                    );
                }
                match engine.advance(&cards) {
                    Progress::Continue => {
                        quiet += 1;
                        // Thousands of steps and nobody asked anything: a loop of triggers.
                        if quiet.is_multiple_of(5_000) {
                            let stack: Vec<String> = engine
                                .view_for(PlayerId(0))
                                .stack
                                .iter()
                                .rev()
                                .take(4)
                                .filter_map(|o| engine.state.objects.get(o))
                                .filter_map(|o| cards.face(o.card, o.face))
                                .map(|f| f.name.to_string())
                                .collect();
                            println!(
                                "QUIET {quiet} steps, {a_name} vs {b_name} game {game}, turn {}, log {}: {stack:?}",
                                engine.state.turn,
                                engine.log.len()
                            );
                        }
                    }
                    Progress::GameOver { .. } => break,
                    Progress::NeedsChoice(c) => {
                        choices += 1;
                        quiet = 0;
                        // A game this long is looping some other way: say where, move on.
                        if choices > 20_000 {
                            println!(
                                "LOOP {a_name} vs {b_name} game {game}, turn {} {:?}: {:?}",
                                engine.state.turn, engine.state.step, c.kind
                            );
                            break;
                        }
                        if engine.state.turn > turns {
                            break;
                        }
                        let view = engine.view_for(c.who);
                        if matches!(c.kind, mtg_engine::ChoiceKind::Priority { .. })
                            && let Some((who, stack, log, action)) = pending.take()
                            && who == c.who
                            && view.stack.len() == stack
                            && engine.log.len() == log
                        {
                            let name = action
                                .play()
                                .map(|(o, _, _)| o)
                                .or(match &action {
                                    Action::ActivateAbility { source, .. } => Some(*source),
                                    _ => None,
                                })
                                .and_then(|o| engine.state.objects.get(&o))
                                .and_then(|o| cards.face(o.card, 0))
                                .map_or_else(|| format!("{action:?}"), |f| f.name.to_string());
                            let entry = found.entry(name).or_insert((0, String::new()));
                            entry.0 += 1;
                            entry.1 = format!(
                                "{a_name} vs {b_name}, turn {}: {action:?}",
                                engine.state.turn
                            );
                        }
                        let answer = bot.decide(&c, &view);
                        if std::env::var_os("SWEEP_TRACE").is_some() {
                            let kind = format!("{:?}", c.kind);
                            eprintln!(
                                "turn {} {:?} {:?}: {} -> {answer:?}",
                                engine.state.turn,
                                engine.state.step,
                                c.who,
                                &kind[..kind.len().min(160)]
                            );
                        }
                        let before = (view.stack.len(), engine.log.len());
                        let accepted = engine.answer(&cards, c.id, answer.clone()).is_ok();
                        if !accepted {
                            println!("REJECTED {:?} -> {answer:?}", c.kind);
                            let _ = engine.answer(&cards, c.id, mtg_policy::well_formed(&c, &view));
                        } else if let Answer::Action(action) = answer
                            && !matches!(action, Action::Concede)
                        {
                            pending = Some((c.who, before.0, before.1, action));
                        }
                    }
                }
            }
            println!(
                "{a_name} vs {b_name} game {game}: turn {}, {choices} choices, {:.1}s",
                engine.state.turn,
                started.elapsed().as_secs_f32()
            );
        }
    }

    println!("{played} games played across {} decks", decks.len());
    if found.is_empty() {
        println!("no action was silently taken back");
    }
    for (name, (n, example)) in &found {
        println!("{n:>5}× {name}\n       {example}");
    }
}
