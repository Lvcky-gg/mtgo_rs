//! Build the self-play deck pool from published MTGO decklists.
//!
//! Self-play and the arena need a field of real decks, not the handful saved locally. MTGO
//! publishes the lists of its leagues and challenges (never the games themselves) as JSON
//! embedded in each event page. This reads the index at `https://www.mtgo.com/decklists/`,
//! fetches the event pages of the chosen formats, resolves every card name against the local
//! card database, and keeps the lists this engine can play — those whose main deck is at least
//! `--min-playable` playable. Unplayable cards are dropped from kept lists, as the arena does.
//!
//! Pages are cached under the pool directory and never fetched twice; requests are spaced a
//! second apart. The pool is written to `~/.local/share/mtgo_rs/selfplay/pool.json`.
//!
//! Run:
//! ```text
//! cargo run --release -p mtg-app --example deck_pool -- [--formats modern,pauper,...]
//!     [--events N] [--min-playable 0.9]
//! ```

use std::{collections::BTreeMap, io::Read, path::PathBuf, time::Duration};

use mtg_app::decks::{LocalSource, open_store};
use mtg_session::game::{CardKey, CardSource, DeckSpec};
use serde::Serialize;
use serde_json::Value;

const INDEX: &str = "https://www.mtgo.com/decklists/";
const MAX_PAGE: u64 = 8 * 1024 * 1024;

/// One published list, as the pool keeps it.
#[derive(Serialize)]
struct Entry {
    /// The format, as the event URL names it: `modern`, `pauper`, …
    format: String,
    event: String,
    date: String,
    source_url: String,
    /// Final rank, when the event published one.
    placement: Option<u32>,
    /// Share of the published main deck (by count) the engine can play.
    playable: f64,
    deck: DeckSpec,
}

fn main() {
    let mut formats: Vec<String> = ["modern", "pauper", "legacy", "pioneer", "standard"]
        .map(String::from)
        .to_vec();
    let mut events = 60usize;
    let mut min_playable = 0.9f64;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_default();
        match flag.as_str() {
            "--formats" => formats = value.split(',').map(|s| s.trim().to_lowercase()).collect(),
            "--events" => events = value.parse().unwrap_or(events),
            "--min-playable" => min_playable = value.parse().unwrap_or(min_playable),
            other => {
                eprintln!("unknown argument {other:?}");
                std::process::exit(2);
            }
        }
    }
    let Some(store) = open_store() else {
        eprintln!("no card database; import cards in the app first");
        std::process::exit(1);
    };
    let source = LocalSource::standard();
    let dir = pool_dir();
    let cache = dir.join("pages");
    std::fs::create_dir_all(&cache).expect("create the pool directory");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(25)))
        .max_redirects(0)
        .build()
        .into();

    // The index is always fetched fresh; event pages never change once published.
    let index = fetch(&agent, INDEX).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let mut links: Vec<String> = Vec::new();
    for tail in index.split("href=\"").skip(1) {
        let Some(path) = tail.split('"').next() else {
            continue;
        };
        let Some(slug) = path.strip_prefix("/decklist/") else {
            continue;
        };
        let wanted = formats.iter().any(|f| slug.starts_with(&format!("{f}-")));
        if wanted && slug.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
            let url = format!("https://www.mtgo.com{path}");
            if !links.contains(&url) {
                links.push(url);
            }
        }
    }
    links.truncate(events);
    println!("{} events to read", links.len());

    let mut names: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut pool = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut per_format: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut missing: BTreeMap<String, u32> = BTreeMap::new();
    for url in &links {
        let file = cache.join(url.rsplit('/').next().unwrap_or("event"));
        let html = match std::fs::read_to_string(&file) {
            Ok(html) => html,
            Err(_) => {
                std::thread::sleep(Duration::from_secs(1));
                match fetch(&agent, url) {
                    Ok(html) => {
                        let _ = std::fs::write(&file, &html);
                        html
                    }
                    Err(e) => {
                        eprintln!("{url}: {e}");
                        continue;
                    }
                }
            }
        };
        let Some(root) = embedded(&html) else {
            eprintln!("{url}: no embedded decklists");
            continue;
        };
        let text = |key: &str| {
            root.get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        // League pages leave `format` blank; the URL always names it.
        let format = url
            .rsplit('/')
            .next()
            .and_then(|slug| slug.split('-').next())
            .unwrap_or("")
            .to_owned();
        let event = text("description");
        let date = text("starttime").chars().take(10).collect::<String>();
        let ranks: BTreeMap<String, u32> = root
            .get("final_rank")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some((
                    r.get("loginid")?.as_str()?.to_owned(),
                    number(r.get("rank")?)?,
                ))
            })
            .collect();
        for list in root
            .get("decklists")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let tally = per_format.entry(format.clone()).or_default();
            tally.0 += 1;
            let mut deck = DeckSpec {
                name: format!(
                    "{} {}",
                    event,
                    list.get("player").and_then(Value::as_str).unwrap_or("?")
                ),
                ..Default::default()
            };
            let (mut total, mut kept) = (0u32, 0u32);
            for (key, section) in [("main_deck", true), ("sideboard_deck", false)] {
                for row in list
                    .get(key)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let Some(name) = row
                        .get("card_attributes")
                        .and_then(|a| a.get("card_name"))
                        .and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let Some(count) = row.get("qty").and_then(number) else {
                        continue;
                    };
                    let count = count.min(255) as u8;
                    let uuid = names
                        .entry(name.to_owned())
                        .or_insert_with(|| resolve(&store, name))
                        .clone();
                    let card = uuid.map(|u| CardKey::Oracle(u, None));
                    let playable = card.as_ref().is_some_and(|k| source.card(k).is_some());
                    if section {
                        total += u32::from(count);
                        if playable {
                            kept += u32::from(count);
                        } else {
                            *missing.entry(name.to_owned()).or_default() += u32::from(count);
                        }
                    }
                    if let (true, Some(card)) = (playable, card) {
                        if section {
                            deck.main.push((card, count));
                        } else {
                            deck.side.push((card, count));
                        }
                    }
                }
            }
            let playable = f64::from(kept) / f64::from(total.max(1));
            // The same list from the same event (a league posts each 5-0 once) counts once.
            let mut fingerprint = deck.main.clone();
            fingerprint.sort();
            if playable < min_playable || kept < 40 || !seen.insert((format.clone(), fingerprint)) {
                continue;
            }
            tally.1 += 1;
            pool.push(Entry {
                format: format.clone(),
                event: event.clone(),
                date: date.clone(),
                source_url: url.clone(),
                placement: list
                    .get("loginid")
                    .and_then(Value::as_str)
                    .and_then(|id| ranks.get(id).copied()),
                playable,
                deck,
            });
        }
    }
    for (format, (lists, kept)) in &per_format {
        println!("{format:<10} {kept:>4} of {lists:>4} lists kept");
    }
    let mut worst: Vec<_> = missing.into_iter().collect();
    worst.sort_by_key(|a| std::cmp::Reverse(a.1));
    println!("most-played unplayable cards (copies across all lists):");
    for (name, n) in worst.iter().take(25) {
        println!("  {n:>5}  {name}");
    }
    let path = dir.join("pool.json");
    std::fs::write(&path, serde_json::to_string_pretty(&pool).unwrap()).expect("write the pool");
    println!("{} decks → {}", pool.len(), path.display());
}

/// Where self-play data lives.
fn pool_dir() -> PathBuf {
    mtg_store::default_path()
        .parent()
        .map(|p| p.join("selfplay"))
        .unwrap_or_else(|| PathBuf::from("selfplay"))
}

fn fetch(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let mut response = agent
        .get(url)
        .header("User-Agent", concat!("mtgo_rs/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| format!("download: {e}"))?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_PAGE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("response: {e}"))?;
    if bytes.len() as u64 > MAX_PAGE {
        return Err("page exceeds 8 MiB".into());
    }
    String::from_utf8(bytes).map_err(|_| "page is not UTF-8".into())
}

/// The event's decklist JSON, embedded in the page as `window.MTGO.decklists.data = {…}`.
fn embedded(html: &str) -> Option<Value> {
    let data = html
        .split_once("window.MTGO.decklists.data")?
        .1
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    serde_json::Deserializer::from_str(data)
        .into_iter()
        .next()?
        .ok()
}

fn number(v: &Value) -> Option<u32> {
    v.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .or_else(|| v.as_str()?.parse().ok())
}

/// The oracle id for an MTGO card name. MTGO writes split cards as `Left/Right`.
fn resolve(store: &mtg_store::Store, name: &str) -> Option<String> {
    let faces: Vec<_> = name
        .split('/')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .collect();
    let name = if name.contains('/') && faces.len() == 2 {
        faces.join(" // ")
    } else {
        name.to_owned()
    };
    let id = store
        .find_exact(&name)
        .ok()
        .flatten()
        .or_else(|| {
            store
                .find_normalised(&mtg_deck::resolve::normalise(&name))
                .ok()
                .flatten()
        })
        .or_else(|| store.find_by_face_name(&name).ok().flatten())?;
    store.card(id).ok().flatten().map(|c| c.oracle_uuid)
}
