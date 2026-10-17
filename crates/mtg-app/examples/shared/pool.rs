//! The self-play deck pool written by the `deck_pool` example, shared by the arena, the
//! evaluation tuner and the self-play recorder.

use mtg_session::game::DeckSpec;
use serde::Deserialize;

#[derive(Deserialize)]
struct Entry {
    format: String,
    deck: DeckSpec,
}

/// Where self-play data lives: beside the card database.
pub fn dir() -> std::path::PathBuf {
    mtg_store::default_path()
        .parent()
        .map(|p| p.join("selfplay"))
        .unwrap_or_else(|| "selfplay".into())
}

/// The pool's decks of `format` (`pauper`, `modern`, …; empty for all), named and without
/// sideboards. Exits with a message when there is no pool yet.
pub fn load(format: &str) -> Vec<(String, DeckSpec)> {
    let path = dir().join("pool.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!(
            "no deck pool at {}; run the deck_pool example first",
            path.display()
        );
        std::process::exit(1);
    };
    let entries: Vec<Entry> = serde_json::from_str(&text).unwrap_or_else(|e| {
        eprintln!("{}: {e}", path.display());
        std::process::exit(1)
    });
    entries
        .into_iter()
        .filter(|e| format.is_empty() || e.format == format)
        .map(|e| {
            let mut deck = e.deck;
            deck.side.clear();
            (deck.name.clone(), deck)
        })
        .collect()
}

/// `count` pairings of distinct decks out of `n`, drawn deterministically from `seed`.
pub fn pairings(n: usize, count: u32, seed: u64) -> Vec<(usize, usize)> {
    let mut x = seed ^ 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        // xorshift64*: deterministic and plenty for picking decks.
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) as usize
    };
    (0..count)
        .filter_map(|_| {
            if n < 2 {
                return None;
            }
            let i = next() % n;
            let j = (i + 1 + next() % (n - 1)) % n;
            Some((i, j))
        })
        .collect()
}
