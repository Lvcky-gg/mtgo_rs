//! Fit the search's evaluation to self-play.
//!
//! Plays the saved decks against each other with the heuristic bot — the same policy search
//! plays its futures out with — and at the start of every turn records both players'
//! [`eval::features`]. Once a game ends, each record is labelled with whether the first player
//! went on to win. The evaluation is `logistic(w · (mine − theirs))`, so fitting `w` is
//! logistic regression; it is solved by Newton's method with a little L2 regularisation.
//!
//! Half the games (by seed parity) are held out. The report compares the held-out log loss of
//! the current weights with the fitted ones; only a lower held-out loss is worth adopting, and
//! only an arena win is worth keeping.
//!
//! Run: `cargo run --release -p mtg-app --example tune_eval -- [--seeds N] [--turns T]`

use std::sync::atomic::{AtomicUsize, Ordering};

use mtg_app::decks::{LocalSource, deck_list, deck_spec, open_store};
use mtg_core::PlayerId;
use mtg_engine::{Engine, Progress};
use mtg_net::fairness::Seed;
use mtg_policy::{
    bot::Bot,
    search::{
        answer_with,
        eval::{self, FEATURES, N, Weights},
    },
};
use mtg_session::game::{CardSource, Format, GameCards, new_game};

/// Feature differences (player 0 − player 1) at one turn start, and whether player 0 won.
type Sample = ([f64; N], f64);

fn main() {
    let mut seeds = 40u32;
    let mut turn_limit = 40u32;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let n = args.next().and_then(|v| v.parse().ok());
        match (a.as_str(), n) {
            ("--seeds", Some(n)) => seeds = n,
            ("--turns", Some(n)) => turn_limit = n,
            _ => {
                eprintln!("usage: tune_eval [--seeds N] [--turns T]");
                std::process::exit(2);
            }
        }
    }
    let weights = Weights::default();
    let store = open_store();
    let source = LocalSource::standard();
    let decks: Vec<_> = deck_list(store.as_ref())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(id, _)| deck_spec(store.as_ref(), id).ok())
        .map(|mut deck| {
            deck.main.retain(|(key, _)| source.card(key).is_some());
            deck.side.clear();
            deck
        })
        .collect();
    let mut tables = Vec::new();
    for i in 0..decks.len() {
        for j in i..decks.len() {
            if decks[i].commander.is_some() != decks[j].commander.is_some() {
                continue;
            }
            let format = if decks[i].commander.is_some() {
                Format::Commander
            } else {
                Format::Constructed
            };
            if let Ok(cards) = GameCards::build([&decks[i], &decks[j]], &source) {
                let bot = Bot::new(&cards, cards.ids());
                tables.push((i, j, format, cards, bot));
            }
        }
    }
    let jobs: Vec<(usize, u32)> = (0..tables.len())
        .flat_map(|t| (0..seeds).map(move |s| (t, s)))
        .collect();
    let next = AtomicUsize::new(0);
    let games = std::sync::Mutex::new(Vec::<(u32, Vec<Sample>)>::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(t, s)) = jobs.get(k) else { break };
                    let (i, j, format, cards, bot) = &tables[t];
                    let seed =
                        Seed([(i * 31 + j * 7) as u8 ^ (s as u8).wrapping_mul(53) ^ 0xA5; 32]);
                    let first = PlayerId((s % 2) as u8);
                    let state = new_game(
                        *format,
                        cards,
                        [&decks[*i], &decks[*j]],
                        &seed,
                        (s % 200) as u8,
                        first,
                    );
                    if let Some(samples) =
                        self_play(Engine::new(state), cards, bot, &weights, turn_limit)
                    {
                        games.lock().unwrap().push((s, samples));
                    }
                }
            });
        }
    });
    let games = games.into_inner().unwrap();
    let (train, test): (Vec<_>, Vec<_>) = games.iter().partition(|(s, _)| s % 2 == 0);
    let flatten = |g: &[&(u32, Vec<Sample>)]| -> Vec<Sample> {
        g.iter().flat_map(|(_, v)| v.iter().copied()).collect()
    };
    let train = flatten(&train);
    let test = flatten(&test);
    println!(
        "{} decided games; {} training and {} held-out samples",
        games.len(),
        train.len(),
        test.len()
    );
    let fitted = fit(&train, 1.0);
    println!(
        "held-out log loss: current {:.4}, fitted {:.4}, coin flip {:.4}",
        log_loss(&test, &weights.coefficients),
        log_loss(&test, &fitted),
        std::f64::consts::LN_2
    );
    println!(
        "held-out accuracy: current {:.1}%, fitted {:.1}%",
        accuracy(&test, &weights.coefficients) * 100.0,
        accuracy(&test, &fitted) * 100.0
    );
    for (name, (old, new)) in FEATURES
        .iter()
        .zip(weights.coefficients.iter().zip(&fitted))
    {
        println!("  {name:<20} {old:>8.3} → {new:>8.3}");
    }
    let literal: Vec<String> = fitted.iter().map(|w| format!("{w:.3}")).collect();
    println!("coefficients: [{}],", literal.join(", "));
}

/// Play one game out heuristically; the samples once it is decided, `None` for a draw or a game
/// stopped at the turn limit.
fn self_play(
    mut engine: Engine,
    cards: &GameCards,
    bot: &Bot,
    weights: &Weights,
    turn_limit: u32,
) -> Option<Vec<Sample>> {
    let mut rows: Vec<[f64; N]> = Vec::new();
    let mut last_turn = 0;
    let mut asked = 0u32;
    let (a, b) = (PlayerId(0), PlayerId(1));
    let winners = loop {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { winners } => break winners,
            Progress::NeedsChoice(c) => {
                asked += 1;
                if engine.state.turn > turn_limit || asked > 40_000 {
                    return None;
                }
                if engine.state.turn != last_turn && engine.state.pregame.is_none() {
                    last_turn = engine.state.turn;
                    let fa = eval::features(&engine.state, cards, a, weights);
                    let fb = eval::features(&engine.state, cards, b, weights);
                    rows.push(std::array::from_fn(|k| fa[k] - fb[k]));
                }
                if !answer_with(bot, &mut engine, cards, &c) {
                    return None;
                }
            }
        }
    };
    let label = match winners.as_slice() {
        [w] if *w == a => 1.0,
        [w] if *w == b => 0.0,
        _ => return None,
    };
    // Both points of view: the evaluation is antisymmetric, so the mirror is a free sample.
    Some(
        rows.into_iter()
            .flat_map(|x| [(x, label), (x.map(|v| -v), 1.0 - label)])
            .collect(),
    )
}

/// L2-regularised logistic regression without an intercept, by Newton's method.
fn fit(data: &[Sample], l2: f64) -> [f64; N] {
    let mut w = [0.0; N];
    for _ in 0..50 {
        let mut grad = [0.0; N];
        let mut hess = [[0.0; N]; N];
        for (x, y) in data {
            let p = eval::logistic(eval::dot(&w, x));
            for i in 0..N {
                grad[i] += (p - y) * x[i];
                for j in 0..N {
                    hess[i][j] += p * (1.0 - p) * x[i] * x[j];
                }
            }
        }
        for i in 0..N {
            grad[i] += l2 * w[i];
            hess[i][i] += l2;
        }
        let step = solve(hess, grad);
        let mut moved = 0.0f64;
        for i in 0..N {
            w[i] -= step[i];
            moved = moved.max(step[i].abs());
        }
        if moved < 1e-7 {
            break;
        }
    }
    w
}

/// Gaussian elimination with partial pivoting.
fn solve(mut a: [[f64; N]; N], mut b: [f64; N]) -> [f64; N] {
    for col in 0..N {
        let pivot = (col..N)
            .max_by(|r, s| a[*r][col].abs().total_cmp(&a[*s][col].abs()))
            .unwrap_or(col);
        a.swap(col, pivot);
        b.swap(col, pivot);
        let d = a[col][col];
        if d.abs() < 1e-12 {
            continue;
        }
        for row in 0..N {
            if row != col {
                let f = a[row][col] / d;
                for k in col..N {
                    a[row][k] -= f * a[col][k];
                }
                b[row] -= f * b[col];
            }
        }
    }
    std::array::from_fn(|i| {
        if a[i][i].abs() < 1e-12 {
            0.0
        } else {
            b[i] / a[i][i]
        }
    })
}

fn log_loss(data: &[Sample], w: &[f64; N]) -> f64 {
    let total: f64 = data
        .iter()
        .map(|(x, y)| {
            let p = eval::logistic(eval::dot(w, x)).clamp(1e-9, 1.0 - 1e-9);
            -(y * p.ln() + (1.0 - y) * (1.0 - p).ln())
        })
        .sum();
    total / data.len().max(1) as f64
}

fn accuracy(data: &[Sample], w: &[f64; N]) -> f64 {
    let right = data
        .iter()
        .filter(|(x, y)| (eval::dot(w, x) > 0.0) == (*y > 0.5))
        .count();
    right as f64 / data.len().max(1) as f64
}
