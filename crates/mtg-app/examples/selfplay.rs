//! Record self-play positions for training a learned evaluation.
//!
//! Plays sampled pairings from the deck pool (`deck_pool` example) with the heuristic bot in
//! both seats. At a random sample of decisions it encodes the position from the deciding
//! player's seat ([`encode::Encoder`]); once the game is decided, every recorded position is
//! labelled with whether that player won. Games stopped at the turn limit are dropped.
//!
//! Each thread writes its own shard under `~/.local/share/mtgo_rs/selfplay/v<schema>/<run>/`:
//!
//! - `positions.bin` — one fixed-size record per position (see `schema.json`).
//! - `tokens.bin` — the positions' visible objects, a contiguous run per position.
//! - `schema.json` — feature names and record layouts, for the training scripts.
//!
//! The opponent's true hand is stored with each position as a label for hand inference. It is
//! never part of the input.
//!
//! Run:
//! ```text
//! cargo run --release -p mtg-app --example selfplay -- [--games N] [--pool FORMAT]
//!     [--sample P] [--turns T] [--threads N] [--run NAME]
//! ```

#[path = "shared/pool.rs"]
mod pool;

use std::{
    io::{BufWriter, Write},
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};

use mtg_app::decks::LocalSource;
use mtg_core::PlayerId;
use mtg_engine::{Engine, Progress};
use mtg_net::fairness::Seed;
use mtg_policy::{
    bot::Bot,
    search::{
        answer_with,
        encode::{self, Encoder},
    },
};
use mtg_session::game::{Format, GameCards, new_game};

/// Opponent hand cards kept as a label per position.
const HIDDEN: usize = 12;

/// One position before its game's result is known.
struct Pending {
    me: PlayerId,
    turn: u32,
    encoded: encode::Encoded,
    hidden: Vec<u32>,
}

fn main() {
    let mut games = 2_000u32;
    let mut format = "pauper".to_owned();
    let mut sample = 0.02f64;
    let mut turns = 60u32;
    let mut threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut run = format!(
        "run-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    );
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_default();
        match flag.as_str() {
            "--games" => games = value.parse().unwrap_or(games),
            "--pool" => format = value.to_lowercase(),
            "--sample" => sample = value.parse().unwrap_or(sample),
            "--turns" => turns = value.parse().unwrap_or(turns),
            "--threads" => threads = value.parse().unwrap_or(threads).max(1),
            "--run" => run = value,
            other => {
                eprintln!("unknown argument {other:?}");
                std::process::exit(2);
            }
        }
    }
    let decks = pool::load(&format);
    let dir = pool::dir()
        .join(format!("v{}", encode::SCHEMA_VERSION))
        .join(&run);
    std::fs::create_dir_all(&dir).expect("create the run directory");
    std::fs::write(dir.join("schema.json"), schema()).expect("write the schema");
    println!(
        "{games} games from {} {format} decks → {}",
        decks.len(),
        dir.display()
    );

    let pairings = pool::pairings(decks.len(), games, 0x5E1F_9A7);
    let next = AtomicUsize::new(0);
    let (decided, positions, tokens) = (AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0));
    let started = std::time::Instant::now();
    std::thread::scope(|scope| {
        for shard in 0..threads {
            let (dir, decks, pairings) = (&dir, &decks, &pairings);
            let (next, decided, positions, tokens) = (&next, &decided, &positions, &tokens);
            scope.spawn(move || {
                let open = |name: &str| {
                    BufWriter::new(
                        std::fs::File::create(dir.join(format!("{shard:02}-{name}")))
                            .expect("create a shard file"),
                    )
                };
                let mut out_positions = open("positions.bin");
                let mut out_tokens = open("tokens.bin");
                let mut written_tokens = 0u64;
                let mut encoder = Encoder::new();
                // The card database connection is per thread.
                let source = &LocalSource::standard();
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(i, j)) = pairings.get(k) else {
                        break;
                    };
                    let (a, b) = (&decks[i].1, &decks[j].1);
                    let Ok(cards) = GameCards::build([a, b], source) else {
                        continue;
                    };
                    let bot = Bot::new(&cards, cards.ids());
                    let mut bytes = [0u8; 32];
                    bytes[..8].copy_from_slice(&(k as u64).to_le_bytes());
                    bytes[8] = 0x5E;
                    let first = PlayerId((k % 2) as u8);
                    let state = new_game(
                        Format::Constructed,
                        &cards,
                        [a, b],
                        &Seed(bytes),
                        (k % 200) as u8,
                        first,
                    );
                    let mut rng = mtg_engine::state::Rng::from_seed(&bytes);
                    let Some((winner, pending)) = play(
                        Engine::new(state),
                        &cards,
                        &bot,
                        &mut encoder,
                        &mut rng,
                        sample,
                        turns,
                    ) else {
                        continue;
                    };
                    decided.fetch_add(1, Ordering::Relaxed);
                    for p in pending {
                        let outcome = match winner {
                            Some(w) if w == p.me => 1.0f32,
                            Some(_) => 0.0,
                            None => 0.5,
                        };
                        // positions.bin record: global f32[NG], token start u64, token count
                        // u32, outcome f32, game u32, turn u32, seat u32, hidden u32[HIDDEN].
                        let mut rec = Vec::with_capacity(encode::NG * 4 + 64);
                        for g in p.encoded.global {
                            rec.extend(g.to_le_bytes());
                        }
                        rec.extend(written_tokens.to_le_bytes());
                        rec.extend((p.encoded.tokens.len() as u32).to_le_bytes());
                        rec.extend(outcome.to_le_bytes());
                        rec.extend((k as u32).to_le_bytes());
                        rec.extend(p.turn.to_le_bytes());
                        rec.extend(u32::from(p.me.0).to_le_bytes());
                        for h in 0..HIDDEN {
                            rec.extend(p.hidden.get(h).copied().unwrap_or(0).to_le_bytes());
                        }
                        out_positions.write_all(&rec).expect("write a position");
                        // tokens.bin record: slot u32, name u32, features f16[NC].
                        for t in &p.encoded.tokens {
                            let mut rec = Vec::with_capacity(encode::NC * 4 + 8);
                            rec.extend((t.slot as u32).to_le_bytes());
                            rec.extend(t.name.to_le_bytes());
                            for f in t.features {
                                rec.extend(f16(f).to_le_bytes());
                            }
                            out_tokens.write_all(&rec).expect("write a token");
                        }
                        written_tokens += p.encoded.tokens.len() as u64;
                        tokens.fetch_add(p.encoded.tokens.len() as u64, Ordering::Relaxed);
                        positions.fetch_add(1, Ordering::Relaxed);
                    }
                    if k % 200 == 0 {
                        println!(
                            "{k:>6} games, {} decided, {} positions, {:.0}s",
                            decided.load(Ordering::Relaxed),
                            positions.load(Ordering::Relaxed),
                            started.elapsed().as_secs_f64()
                        );
                    }
                }
                out_positions.flush().expect("flush positions");
                out_tokens.flush().expect("flush tokens");
            });
        }
    });
    println!(
        "done: {} decided games, {} positions, {} tokens, {:.0}s",
        decided.load(Ordering::Relaxed),
        positions.load(Ordering::Relaxed),
        tokens.load(Ordering::Relaxed),
        started.elapsed().as_secs_f64()
    );
}

/// Play one game heuristically. The winner (`None` for a draw) and the sampled positions, or
/// `None` when the game was stopped undecided.
fn play(
    mut engine: Engine,
    cards: &GameCards,
    bot: &Bot,
    encoder: &mut Encoder,
    rng: &mut mtg_engine::state::Rng,
    sample: f64,
    turns: u32,
) -> Option<(Option<PlayerId>, Vec<Pending>)> {
    let mut pending = Vec::new();
    let mut asked = 0u32;
    loop {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { winners } => {
                let winner = match winners.as_slice() {
                    [w] => Some(*w),
                    _ => None,
                };
                return Some((winner, pending));
            }
            Progress::NeedsChoice(c) => {
                asked += 1;
                if engine.state.turn > turns || asked > 20_000 {
                    return None;
                }
                let threshold = (sample * u64::MAX as f64) as u64;
                if engine.state.pregame.is_none() && rng.next_u64() < threshold {
                    let opponent = engine
                        .state
                        .players
                        .keys()
                        .copied()
                        .find(|p| *p != c.who)
                        .unwrap_or(c.who);
                    pending.push(Pending {
                        me: c.who,
                        turn: engine.state.turn,
                        encoded: encoder.encode(&engine.state, cards, c.who),
                        hidden: encode::hidden_hand(&engine.state, cards, opponent),
                    });
                }
                if !answer_with(bot, &mut engine, cards, &c) {
                    return None;
                }
            }
        }
    }
}

/// IEEE half precision, rounded to nearest: the token features are small counts and flags,
/// and half the bytes is half the disk.
fn f16(x: f32) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32 - 127 + 15;
    let mantissa = bits & 0x7F_FFFF;
    if x.is_nan() {
        return sign | 0x7E00;
    }
    if exp >= 31 {
        return sign | 0x7C00;
    }
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let m = (mantissa | 0x80_0000) >> (1 - exp);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    let rounded = ((exp as u32) << 10 | mantissa >> 13) + ((mantissa >> 12) & 1);
    sign | rounded as u16
}

/// `schema.json`: what the binary files hold, for the training scripts.
fn schema() -> String {
    let global: Vec<String> = encode::GLOBAL
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let side = if i < encode::GLOBAL.len() / 2 && i < 11 {
                "my "
            } else if i < 22 {
                "their "
            } else {
                ""
            };
            format!("{side}{n}")
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": encode::SCHEMA_VERSION,
        "ng": encode::NG,
        "nc": encode::NC,
        "slots": encode::SLOTS,
        "hidden": HIDDEN,
        "global": global,
        "card": encode::card_features(),
        "positions": [
            ["global", "f32", encode::NG],
            ["token_start", "u64", 1],
            ["token_count", "u32", 1],
            ["outcome", "f32", 1],
            ["game", "u32", 1],
            ["turn", "u32", 1],
            ["seat", "u32", 1],
            ["hidden", "u32", HIDDEN],
        ],
        "tokens": [
            ["slot", "u32", 1],
            ["name", "u32", 1],
            ["features", "f16", encode::NC],
        ],
    }))
    .unwrap()
}
