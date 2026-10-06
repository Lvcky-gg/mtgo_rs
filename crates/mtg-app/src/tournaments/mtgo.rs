//! Official MTGO event-page adapter. The embedded JSON is parsed, never executed.
use serde_json::{Value, json};
use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};

const INDEX: &str = "https://www.mtgo.com/decklists/";
const EVENT_PREFIX: &str = "https://www.mtgo.com/decklist/modern-";
const MAX_PAGE: usize = 8 * 1024 * 1024;
const MAX_EVENTS: usize = 5;

pub(crate) struct Fetch {
    receiver: Receiver<Result<String, String>>,
    cancelled: Arc<AtomicBool>,
}
impl Drop for Fetch {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
impl Fetch {
    pub(crate) fn start(ctx: egui::Context, url: String) -> Self {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        std::thread::spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(25)))
                .max_redirects(0)
                .build()
                .into();
            let result = load(
                &url,
                |url| {
                    let mut response = agent
                        .get(url)
                        .header("User-Agent", concat!("mtgo_rs/", env!("CARGO_PKG_VERSION")))
                        .call()
                        .map_err(|e| format!("MTGO download: {e}"))?;
                    let mut bytes = Vec::new();
                    response
                        .body_mut()
                        .as_reader()
                        .take(MAX_PAGE as u64 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|e| format!("MTGO response: {e}"))?;
                    if bytes.len() > MAX_PAGE {
                        return Err("MTGO page exceeds 8 MiB.".into());
                    }
                    String::from_utf8(bytes).map_err(|_| "MTGO page is not UTF-8.".into())
                },
                || flag.load(Ordering::Relaxed),
            );
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        Self {
            receiver,
            cancelled,
        }
    }
    pub(crate) fn poll(&self) -> Option<Result<String, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "Tournament download stopped before returning a result.".into(),
            )),
        }
    }
}

fn event_url(url: &str) -> bool {
    url.starts_with(EVENT_PREFIX)
        && url.len() < 512
        && url[EVENT_PREFIX.len()..]
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        && url.len() > EVENT_PREFIX.len()
}

fn event_links(html: &str) -> Vec<String> {
    let mut result = Vec::new();
    for tail in html.split("href=\"").skip(1) {
        let Some(path) = tail.split('"').next() else {
            continue;
        };
        if !path.starts_with("/decklist/modern-challenge-") {
            continue;
        }
        let url = format!("https://www.mtgo.com{path}");
        if event_url(&url) && !result.contains(&url) {
            result.push(url);
        }
        if result.len() == MAX_EVENTS {
            break;
        }
    }
    result
}

fn quantity(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|n| (1..=255).contains(n))
}

fn parse_event(html: &str, url: &str) -> Result<Vec<Value>, String> {
    let tail = html
        .split_once("window.MTGO.decklists.data")
        .ok_or("MTGO page has no embedded decklist data; the site format may have changed.")?
        .1;
    let data = tail
        .trim_start()
        .strip_prefix('=')
        .ok_or("Unrecognized MTGO decklist assignment.")?
        .trim_start();
    let root: Value = serde_json::Deserializer::from_str(data)
        .into_iter()
        .next()
        .ok_or("Missing MTGO JSON.")?
        .map_err(|e| format!("MTGO embedded JSON: {e}"))?;
    if root.get("format").and_then(Value::as_str) != Some("CMODERN") {
        return Err("This event is not Modern.".into());
    }
    let text = |key: &str| {
        root.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("Missing MTGO {key}."))
    };
    let event = text("description")?;
    let event_id = text("event_id")?;
    let date = text("starttime")?
        .get(..10)
        .ok_or("Invalid MTGO event date.")?;
    let decks = root
        .get("decklists")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty() && rows.len() <= 100)
        .ok_or("MTGO event needs 1–100 published lists.")?;
    let mut records = Vec::new();
    for deck in decks {
        let player = deck
            .get("loginid")
            .and_then(Value::as_str)
            .ok_or("Missing MTGO player identity.")?;
        let mut placement = None;
        let mut last_round = 0;
        if let Some(ranks) = root.get("final_rank").and_then(Value::as_array) {
            for rank in ranks {
                if rank.get("loginid").and_then(Value::as_str) == Some(player) {
                    let round = rank.get("roundnumber").and_then(quantity).unwrap_or(0);
                    if round >= last_round {
                        last_round = round;
                        placement = rank.get("rank").and_then(quantity);
                    }
                }
            }
        }
        let cards = |key: &str| -> Result<Vec<Value>, String> {
            let rows = deck
                .get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| format!("Missing MTGO {key}."))?;
            if rows.len() > 200 {
                return Err("Too many MTGO card entries.".into());
            }
            rows.iter()
                .map(|row| {
                    let name = row
                        .get("card_attributes")
                        .and_then(|a| a.get("card_name"))
                        .and_then(Value::as_str)
                        .ok_or("Missing MTGO card name.")?;
                    // MTGO exports split cards as "Left/Right"; oracle names use
                    // "Left // Right". Only two nonempty faces receive this adapter.
                    let faces: Vec<_> = name
                        .split('/')
                        .map(str::trim)
                        .filter(|face| !face.is_empty())
                        .collect();
                    let name = if name.contains('/') && faces.len() == 2 {
                        faces.join(" // ")
                    } else {
                        name.to_owned()
                    };
                    let count = row
                        .get("qty")
                        .and_then(quantity)
                        .ok_or("Invalid MTGO card quantity.")?;
                    Ok(json!({"name": name, "count": count}))
                })
                .collect()
        };
        records.push(json!({"id":format!("mtgo/{event_id}/{player}"), "event":event, "date":date, "format":"modern", "source_url":url, "placement":placement, "main":cards("main_deck")?, "sideboard":cards("sideboard_deck")?}));
    }
    Ok(records)
}

fn load(
    url: &str,
    mut fetch: impl FnMut(&str) -> Result<String, String>,
    cancelled: impl Fn() -> bool,
) -> Result<String, String> {
    let url = url.trim();
    if !url.is_empty() && !event_url(url) {
        return Err("Use an official https://www.mtgo.com/decklist/modern-… event URL, or leave blank for the latest five Modern Challenges.".into());
    }
    if cancelled() {
        return Err("Tournament download cancelled.".into());
    }
    let events = if url.is_empty() {
        let index = fetch(INDEX)?;
        let links = event_links(&index);
        if links.is_empty() {
            return Err(
                "No Modern Challenge links found; the MTGO site format may have changed.".into(),
            );
        }
        links
    } else {
        vec![url.to_owned()]
    };
    let mut records = Vec::new();
    for event in events {
        if cancelled() {
            return Err("Tournament download cancelled.".into());
        }
        records.extend(parse_event(&fetch(&event)?, &event)?);
    }
    if cancelled() {
        return Err("Tournament download cancelled.".into());
    }
    serde_json::to_string(&records).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page() -> String {
        format!(
            "<script>window.MTGO.decklists.data = {}; window.after = 1;</script>",
            json!({"event_id":"123", "description":"Modern Challenge", "starttime":"2026-10-05 01:00:00.0", "format":"CMODERN", "decklists":[{"loginid":"456", "main_deck":[{"qty":"4","card_attributes":{"card_name":"Invented Scout"}}],"sideboard_deck":[]}],"final_rank":[{"loginid":"456","rank":"8","roundnumber":"4"}]})
        )
    }
    #[test]
    fn embedded_json_preserves_identity_quantities_source_and_finish() {
        let records = parse_event(&page(), "https://www.mtgo.com/decklist/modern-test").unwrap();
        assert_eq!(records[0]["id"], "mtgo/123/456");
        assert_eq!(records[0]["placement"], 8);
        assert_eq!(records[0]["main"][0]["count"], 4);
        assert_eq!(records[0]["date"], "2026-10-05");
        let split = parse_event(
            &page().replace("Invented Scout", "Invented Dawn/Invented Dusk"),
            "url",
        )
        .unwrap();
        assert_eq!(
            split[0]["main"][0]["name"],
            "Invented Dawn // Invented Dusk"
        );
        assert!(parse_event(&page().replace("CMODERN", "CSTANDARD"), "url").is_err());
        assert!(parse_event("<script>alert(1)</script>", "url").is_err());
    }
    #[test]
    fn discovery_is_bounded_deduplicated_and_rejects_external_urls() {
        let mut index = String::new();
        for i in 0..10 {
            index.push_str(&format!(
                "<a href=\"/decklist/modern-challenge-{i}\">event</a>"
            ));
        }
        assert_eq!(event_links(&index).len(), 5);
        let mut calls = Vec::new();
        let output = load(
            "",
            |url| {
                calls.push(url.to_owned());
                Ok(if url == INDEX { index.clone() } else { page() })
            },
            || false,
        )
        .unwrap();
        assert_eq!(calls.len(), 6);
        assert_eq!(
            serde_json::from_str::<Vec<Value>>(&output).unwrap().len(),
            5
        );
        for url in [
            "https://evil.test/decklist/modern-test",
            "https://www.mtgo.com/decklist/modern-../x",
            "http://www.mtgo.com/decklist/modern-test",
        ] {
            assert!(load(url, |_| panic!("invalid URL fetched"), || false).is_err());
        }
        assert!(load("", |_| panic!("cancelled request fetched"), || true).is_err());
    }
    #[test]
    fn cancellation_after_discovery_stops_event_requests_and_disconnected_workers_report_errors() {
        let cancelled = std::cell::Cell::new(false);
        let mut calls = 0;
        let result = load(
            "",
            |_| {
                calls += 1;
                cancelled.set(true);
                Ok("<a href=\"/decklist/modern-challenge-test\">event</a>".into())
            },
            || cancelled.get(),
        );
        assert!(result.is_err());
        assert_eq!(calls, 1);
        let (sender, receiver) = mpsc::channel();
        drop(sender);
        let fetch = Fetch {
            receiver,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        assert!(fetch.poll().unwrap().unwrap_err().contains("stopped"));
        assert!(parse_event(&page().replace("\"qty\":\"4\"", "\"qty\":\"-1\""), "url").is_err());
    }
    #[test]
    #[ignore = "read-only live MTGO compatibility check; requires network"]
    fn live_event_schema_still_parses() {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(25)))
            .max_redirects(0)
            .build()
            .into();
        let url = if std::env::var_os("MTGO_TEST_LATEST").is_some() {
            ""
        } else {
            "https://www.mtgo.com/decklist/modern-challenge-64-2026-10-0512855498"
        };
        let result = load(
            url,
            |url| {
                agent
                    .get(url)
                    .call()
                    .map_err(|e| e.to_string())?
                    .body_mut()
                    .read_to_string()
                    .map_err(|e| e.to_string())
            },
            || false,
        )
        .unwrap();
        assert!(
            !serde_json::from_str::<Vec<Value>>(&result)
                .unwrap()
                .is_empty()
        );
        // Optional card-database snapshot verifies name resolution and persistence without
        // changing the input database. Supply a consistent SQLite backup, not a live WAL file.
        if let Ok(snapshot) = std::env::var("MTGO_TEST_CARD_SNAPSHOT") {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "mtgo-tournament-live-{}-{nonce}.sqlite",
                std::process::id()
            ));
            struct TemporaryDatabase(std::path::PathBuf);
            impl Drop for TemporaryDatabase {
                fn drop(&mut self) {
                    let _ = std::fs::remove_file(&self.0);
                    for suffix in ["-wal", "-shm"] {
                        let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
                    }
                }
            }
            let temporary = TemporaryDatabase(path.clone());
            std::fs::copy(snapshot, &path).unwrap();
            let store = mtg_store::Store::open(&path).unwrap();
            let imported = super::super::import(&store, &result, super::super::today()).unwrap();
            assert!(imported > 0);
            assert!(super::super::count(&store).unwrap() >= imported);
            drop(store);
            drop(temporary);
        }
    }
}
