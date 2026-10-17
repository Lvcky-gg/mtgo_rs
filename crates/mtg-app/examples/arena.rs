//! The arena: how much stronger is one bot than another?
//!
//! Plays the saved decks against each other, the searching bot ([`Searcher`]) in one seat and
//! the heuristic bot in the other, and reports the searcher's score with a confidence interval.
//! Every seed is played twice with the seats swapped, so the deck, the play/draw and the shuffle
//! are the same for both bots and only their decisions differ (the pairing trick chess engine
//! testing uses). Games that pass the turn limit are scored by the evaluation, as adjudication.
//!
//! This is the yardstick for every change to the bots: a change that does not move the score
//! here did not make them stronger.
//!
//! Run:
//! ```text
//! cargo run --release -p mtg-app --example arena -- [--seeds N] [--playouts P]
//!     [--horizon T] [--margin M] [--turns T] [--pool FORMAT]
//!     [--value PATH/value.bin] [--threads N] [--deck NAME] [--baseline]
//! ```
//! `--baseline` plays the heuristic bot against itself: the score should be 50% and the
//! time per game shows how many playouts a decision can afford.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[path = "shared/pool.rs"]
mod pool;

use mtg_app::decks::{LocalSource, deck_list, deck_spec, open_store};
use mtg_core::PlayerId;
use mtg_engine::{Engine, Progress};
use mtg_net::fairness::Seed;
use mtg_policy::{
    bot::Bot,
    search::{Budget, Searcher, answer_with, eval},
};
use mtg_session::game::{CardSource, Format, GameCards, new_game};

struct Args {
    seeds: u32,
    budget: Budget,
    turns: u32,
    threads: usize,
    deck: Option<String>,
    /// A learned value network (`value.bin`) for the searcher's playout leaves.
    value: Option<std::sync::Arc<mtg_policy::search::value::ValueNet>>,
    /// Play sampled pairings from the self-play deck pool, of this format.
    pool: Option<String>,
    baseline: bool,
    /// Evaluation coefficients to search with, instead of the defaults.
    coefficients: Option<[f64; eval::N]>,
}

fn parse() -> Args {
    let mut a = Args {
        seeds: 8,
        budget: Budget::default(),
        turns: 25,
        threads: std::thread::available_parallelism().map_or(4, |n| n.get()),
        deck: None,
        pool: None,
        value: None,
        baseline: false,
        coefficients: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut num = || it.next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        match flag.as_str() {
            "--seeds" => a.seeds = num(),
            "--playouts" => a.budget.playouts = num(),
            "--horizon" => a.budget.horizon_turns = num(),
            "--margin" => a.budget.margin = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0),
            "--turns" => a.turns = num(),
            "--threads" => a.threads = num().max(1) as usize,
            "--deck" => a.deck = it.next().map(|s| s.to_lowercase()),
            "--value" => {
                let path = it.next().unwrap_or_default();
                match mtg_policy::search::value::ValueNet::load(path.as_ref()) {
                    Ok(net) => a.value = Some(std::sync::Arc::new(net)),
                    Err(e) => {
                        eprintln!("--value: {e}");
                        std::process::exit(2);
                    }
                }
            }
            "--pool" => a.pool = Some(it.next().unwrap_or_default().to_lowercase()),
            "--baseline" => a.baseline = true,
            "--coefficients" => {
                let parsed: Vec<f64> = it
                    .next()
                    .unwrap_or_default()
                    .trim_matches(|c| c == '[' || c == ']' || c == ',')
                    .split(',')
                    .filter_map(|v| v.trim().parse().ok())
                    .collect();
                match <[f64; eval::N]>::try_from(parsed) {
                    Ok(c) => a.coefficients = Some(c),
                    Err(_) => {
                        eprintln!("--coefficients needs {} comma-separated numbers", eval::N);
                        std::process::exit(2);
                    }
                }
            }
            other => {
                eprintln!("unknown argument {other:?}");
                std::process::exit(2);
            }
        }
    }
    a
}

/// One game's result from the searcher's point of view: 1, ½ or 0.
struct Played {
    score: f64,
    adjudicated: bool,
    turns: u32,
    decisions: u64,
}

#[allow(clippy::too_many_arguments)]
fn play(
    cards: &GameCards,
    format: Format,
    decks: [&mtg_session::game::DeckSpec; 2],
    seed: &Seed,
    game: u8,
    search_seat: PlayerId,
    bot: &Bot,
    searcher: &Searcher,
    args: &Args,
    search_nanos: &AtomicU64,
) -> Played {
    let mut engine = Engine::new(new_game(format, cards, decks, seed, game, PlayerId(0)));
    let mut decisions = 0;
    let mut asked = 0u32;
    loop {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { winners } => {
                let score = match (winners.contains(&search_seat), winners.len()) {
                    (true, 1) => 1.0,
                    (false, _) => 0.0,
                    _ => 0.5,
                };
                return Played {
                    score,
                    adjudicated: false,
                    turns: engine.state.turn,
                    decisions,
                };
            }
            Progress::NeedsChoice(c) => {
                asked += 1;
                if engine.state.turn > args.turns || asked > 5_000 {
                    let p = eval::evaluate(
                        &engine.state,
                        cards,
                        search_seat,
                        &eval::Weights::default(),
                    );
                    return Played {
                        score: if p > 0.6 {
                            1.0
                        } else if p < 0.4 {
                            0.0
                        } else {
                            0.5
                        },
                        adjudicated: true,
                        turns: engine.state.turn,
                        decisions,
                    };
                }
                // A looping game would otherwise search every one of thousands of decisions;
                // past this many the searcher plays the rest heuristically.
                if c.who == search_seat && !args.baseline && decisions < SEARCHED_PER_GAME {
                    let started = Instant::now();
                    let answer = searcher.decide(&engine, cards, &c);
                    search_nanos.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
                    let heuristic = searcher.bot.decide(&c, &engine.view_for(c.who));
                    if format!("{heuristic:?}") != format!("{answer:?}") {
                        OVERRIDES.fetch_add(1, Ordering::Relaxed);
                        if std::env::var_os("ARENA_TRACE").is_some() {
                            eprintln!(
                                "turn {} {:?}: heuristic {heuristic:?} -> search {answer:?}",
                                engine.state.turn, engine.state.step
                            );
                        }
                    }
                    decisions += 1;
                    let view = engine.view_for(c.who);
                    if engine.answer(cards, c.id, answer).is_err()
                        && engine
                            .answer(cards, c.id, mtg_policy::well_formed(&c, &view))
                            .is_err()
                    {
                        return Played {
                            score: 0.5,
                            adjudicated: true,
                            turns: engine.state.turn,
                            decisions,
                        };
                    }
                } else if !answer_with(bot, &mut engine, cards, &c) {
                    return Played {
                        score: 0.5,
                        adjudicated: true,
                        turns: engine.state.turn,
                        decisions,
                    };
                }
            }
        }
    }
}

/// Searched decisions per game before the searcher falls back to the heuristic.
const SEARCHED_PER_GAME: u64 = 600;

/// Decisions where the search chose something other than the heuristic.
static OVERRIDES: AtomicU64 = AtomicU64::new(0);

fn elo(score: f64) -> f64 {
    let s = score.clamp(1e-3, 1.0 - 1e-3);
    -400.0 * (1.0 / s - 1.0).log10()
}

fn main() {
    let args = parse();
    let store = open_store();
    let source = LocalSource::standard();
    let saved = || {
        deck_list(store.as_ref())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(id, name)| deck_spec(store.as_ref(), id).ok().map(|d| (name, d)))
            .filter(|(name, _)| {
                args.deck
                    .as_ref()
                    .is_none_or(|d| name.to_lowercase().contains(d))
            })
            .map(|(name, mut deck)| {
                // Cards the compiler can't play yet are left out, as in the rewind sweep.
                let before = deck.main.iter().map(|(_, n)| u32::from(*n)).sum::<u32>();
                deck.main.retain(|(key, _)| source.card(key).is_some());
                let after = deck.main.iter().map(|(_, n)| u32::from(*n)).sum::<u32>();
                deck.side.clear();
                println!("deck {name}: {after}/{before} cards playable");
                (name, deck)
            })
            .collect::<Vec<_>>()
    };
    // Saved decks: every pair of the same format, and each deck against itself, on every seed.
    // The pool: one sampled pairing of distinct decks per seed, so the field is covered.
    let (decks, pairings): (Vec<_>, Vec<(usize, usize, Vec<u32>)>) = match &args.pool {
        Some(format) => {
            let decks = pool::load(format);
            println!("pool {format:?}: {} decks", decks.len());
            let pairings = pool::pairings(decks.len(), args.seeds, 0xA4E7A)
                .into_iter()
                .zip(0..)
                .map(|((i, j), s)| (i, j, vec![s]))
                .collect();
            (decks, pairings)
        }
        None => {
            let decks = saved();
            let mut pairings = Vec::new();
            for i in 0..decks.len() {
                for j in i..decks.len() {
                    if decks[i].1.commander.is_some() != decks[j].1.commander.is_some() {
                        continue;
                    }
                    pairings.push((i, j, (0..args.seeds).collect()));
                }
            }
            (decks, pairings)
        }
    };
    let search_nanos = AtomicU64::new(0);
    let started = Instant::now();
    // Card tables first, so every game of every pairing can share one pool of threads.
    let mut tables = Vec::new();
    for (i, j, seeds) in pairings {
        let ((a_name, a), (b_name, b)) = (&decks[i], &decks[j]);
        let format = if a.commander.is_some() {
            Format::Commander
        } else {
            Format::Constructed
        };
        let Ok(cards) = GameCards::build([a, b], &source) else {
            println!("skip {a_name} vs {b_name}: unplayable cards");
            continue;
        };
        let bot = Bot::new(&cards, cards.ids());
        tables.push((i, j, format, cards, bot, seeds));
    }
    // Each job is one seed of one pairing: both seat assignments, so the pair's mean cancels
    // the deal.
    let jobs: Vec<(usize, u32)> = tables
        .iter()
        .enumerate()
        .flat_map(|(t, table)| table.5.iter().map(move |s| (t, *s)))
        .collect();
    let finished = std::sync::Mutex::new(Vec::new());
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..args.threads.min(jobs.len()) {
            scope.spawn(|| {
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(t, s)) = jobs.get(k) else { break };
                    let (i, j, format, cards, bot, _) = &tables[t];
                    let seed = Seed([(i * 31 + j * 7) as u8 ^ (s as u8).wrapping_mul(101); 32]);
                    let mut searcher = Searcher::new(
                        Bot::new(cards, cards.ids()),
                        args.budget.clone(),
                        u64::from(s) * 0x51_7CC1,
                    );
                    searcher.value = args.value.clone();
                    if let Some(c) = args.coefficients {
                        searcher.weights.coefficients = c;
                    }
                    let pair: Vec<Played> = [PlayerId(0), PlayerId(1)]
                        .into_iter()
                        .map(|seat| {
                            play(
                                cards,
                                *format,
                                [&decks[*i].1, &decks[*j].1],
                                &seed,
                                (s % 200) as u8,
                                seat,
                                bot,
                                &searcher,
                                &args,
                                &search_nanos,
                            )
                        })
                        .collect();
                    let mut done = finished.lock().unwrap();
                    done.push((t, pair));
                    if done.len() % 8 == 0 {
                        eprintln!(
                            "{}/{} seed pairs, {:.0}s",
                            done.len(),
                            jobs.len(),
                            started.elapsed().as_secs_f64()
                        );
                    }
                }
            });
        }
    });
    let finished = finished.into_inner().unwrap();
    let mut all: Vec<f64> = Vec::new();
    let (mut adjudicated, mut games, mut decisions, mut turns) = (0, 0, 0u64, 0u64);
    for (t, (i, j, ..)) in tables.iter().enumerate() {
        let mut means = Vec::new();
        for (_, pair) in finished.iter().filter(|(u, _)| *u == t) {
            for p in pair {
                games += 1;
                adjudicated += u32::from(p.adjudicated);
                decisions += p.decisions;
                turns += u64::from(p.turns);
            }
            means.push(pair.iter().map(|p| p.score).sum::<f64>() / pair.len() as f64);
        }
        let mean = means.iter().sum::<f64>() / means.len().max(1) as f64;
        all.extend(&means);
        if args.pool.is_some() {
            continue;
        }
        println!(
            "{} vs {}: searcher {:.1}% over {} games",
            decks[*i].0,
            decks[*j].0,
            mean * 100.0,
            means.len() * 2
        );
    }

    let n = all.len().max(1) as f64;
    let mean = all.iter().sum::<f64>() / n;
    let var = all.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    let half = 1.96 * (var / n).sqrt();
    println!(
        "\n── {} ──",
        if args.baseline {
            "baseline: bot vs bot"
        } else {
            "searcher vs bot"
        }
    );
    println!(
        "score      {:.1}% ± {:.1}%  ({games} games, {} seed pairs)",
        mean * 100.0,
        half * 100.0,
        all.len()
    );
    println!(
        "elo        {:+.0}  [{:+.0}, {:+.0}]",
        elo(mean),
        elo(mean - half),
        elo(mean + half)
    );
    println!(
        "adjudicated {adjudicated}/{games}, mean length {:.1} turns",
        turns as f64 / games.max(1) as f64
    );
    let secs = started.elapsed().as_secs_f64();
    println!(
        "wall time  {secs:.1}s, {:.2}s per game",
        secs / games.max(1) as f64
    );
    if decisions > 0 {
        println!(
            "searched   {decisions} decisions, {:.1} ms each (playouts {}, horizon {}), {} overrode the heuristic",
            search_nanos.load(Ordering::Relaxed) as f64 / decisions as f64 / 1e6,
            args.budget.playouts,
            args.budget.horizon_turns,
            OVERRIDES.load(Ordering::Relaxed)
        );
    }
}
