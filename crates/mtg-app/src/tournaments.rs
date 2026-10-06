//! Offline observations from published tournament lists, not card win-rate estimates.
use crate::builder::{Draft, DraftCard};
use mtg_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) mod mtgo;

const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_RECORDS: usize = 2000;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NamedCard {
    name: String,
    count: u32,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    event: String,
    date: String,
    format: String,
    source_url: String,
    #[serde(default)]
    placement: Option<u32>,
    main: Vec<NamedCard>,
    #[serde(default)]
    sideboard: Vec<NamedCard>,
}

pub(crate) struct Pick {
    pub oracle: u32,
    pub name: String,
    pub sideboard: bool,
    pub copies: u32,
    pub included: usize,
    pub samples: usize,
    pub first_date: String,
    pub last_date: String,
    pub sources: Vec<(String, String)>,
    pub score: f64,
}

#[derive(Default)]
pub(crate) struct Benchmark {
    pub samples: usize,
    pub curve: [f64; 8],
    pub lands: f64,
    pub average_mana: f64,
}

pub(crate) struct Report {
    pub picks: Vec<Pick>,
    pub benchmark: Benchmark,
}

pub(crate) fn today() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        / 86400
}

// Gregorian civil date to days since 1970; validate first rather than normalizing invalid dates.
fn day(date: &str) -> Option<i64> {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return None;
    }
    let year: i64 = date[..4].parse().ok()?;
    let month: i64 = date[5..7].parse().ok()?;
    let date_day: i64 = date[8..].parse().ok()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let length = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return None,
    };
    if !(1993..=9999).contains(&year) || !(1..=length).contains(&date_day) {
        return None;
    }
    let year = year - i64::from(month <= 2);
    let era = year / 400;
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + date_day - 1;
    Some(era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468)
}

fn resolve(
    store: &Store,
    cards: &[NamedCard],
    names: &mut BTreeMap<String, u32>,
) -> Result<BTreeMap<u32, u32>, String> {
    let mut result = BTreeMap::<u32, u32>::new();
    for card in cards {
        let name = card.name.trim();
        let id = if let Some(id) = names.get(name) {
            *id
        } else {
            let id = match store.find_exact(name).map_err(|e| e.to_string())? {
                Some(id) => id,
                None => store
                    .find_normalised(&mtg_deck::resolve::normalise(name))
                    .map_err(|e| e.to_string())?
                    .or(store.find_by_face_name(name).map_err(|e| e.to_string())?)
                    .ok_or_else(|| format!("Unknown tournament card: {name}"))?,
            };
            names.insert(name.to_owned(), id);
            id
        };
        let count = result.entry(id).or_default();
        *count = count
            .checked_add(card.count)
            .filter(|n| *n <= 255)
            .ok_or_else(|| format!("Too many copies of {}", card.name))?;
    }
    Ok(result)
}

fn validate(record: &Record, now: i64) -> Result<(), String> {
    if record.id.trim().is_empty()
        || record.id.len() > 256
        || record.event.trim().is_empty()
        || record.event.len() > 512
    {
        return Err("Every result needs an id and event name (maximum 256/512 bytes).".into());
    }
    if record.format != "modern" {
        return Err("Tournament imports currently support format modern only.".into());
    }
    if day(&record.date).is_none_or(|day| day > now) {
        return Err(format!("Invalid or future event date: {}", record.date));
    }
    if record.source_url.len() > 2048
        || !(record.source_url.starts_with("https://") || record.source_url.starts_with("http://"))
        || record.source_url.chars().any(char::is_whitespace)
    {
        return Err("Each result needs an HTTP(S) source_url.".into());
    }
    if record.placement == Some(0) {
        return Err("Placement must be positive when supplied.".into());
    }
    if record.main.is_empty() || record.main.len() > 200 || record.sideboard.len() > 200 {
        return Err("Each result needs a main deck and at most 200 entries per section.".into());
    }
    for card in record.main.iter().chain(&record.sideboard) {
        if card.name.trim().is_empty() || card.name.len() > 256 || !(1..=255).contains(&card.count)
        {
            return Err("Invalid tournament card name or quantity (1–255 required).".into());
        }
    }
    Ok(())
}

/// Validate the complete batch before writing; repeated ids update rather than add observations.
pub(crate) fn import(store: &Store, input: &str, now: i64) -> Result<usize, String> {
    if input.len() > MAX_BYTES {
        return Err("Tournament file exceeds 2 MiB.".into());
    }
    let records: Vec<Record> = serde_json::from_str(input.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("Tournament JSON: {e}"))?;
    if records.is_empty() || records.len() > 500 {
        return Err("Import between 1 and 500 results at a time.".into());
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeMap::new();
    for record in &records {
        validate(record, now)?;
        if !ids.insert(&record.id) {
            return Err(format!("Duplicate result id: {}", record.id));
        }
        resolve(store, &record.main, &mut names)?;
        resolve(store, &record.sideboard, &mut names)?;
    }
    let tx = store
        .conn()
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    for record in &records {
        tx.execute("INSERT INTO tournament_records(id,payload) VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", mtg_store::rusqlite::params![record.id, serde_json::to_string(record).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
    }
    let total: u32 = tx
        .query_row("SELECT count(*) FROM tournament_records", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if total as usize > MAX_RECORDS {
        return Err(format!(
            "The local dataset is limited to {MAX_RECORDS} results."
        ));
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(records.len())
}

pub(crate) fn count(store: &Store) -> Result<usize, String> {
    store
        .conn()
        .query_row("SELECT count(*) FROM tournament_records", [], |r| {
            r.get::<_, u32>(0)
        })
        .map(|n| n as usize)
        .map_err(|e| e.to_string())
}

fn basic(card: &DraftCard) -> bool {
    card.type_line
        .split_whitespace()
        .any(|word| word == "Basic")
        && card.is_land()
}

pub(crate) fn report(
    store: &Store,
    draft: &Draft,
    now: i64,
    limit: usize,
) -> Result<Report, String> {
    let draft_cards: BTreeMap<u32, u32> = draft
        .main
        .iter()
        .filter(|(id, count)| **count > 0 && draft.cards.get(id).is_some_and(|card| !basic(card)))
        .map(|(id, n)| (*id, *n))
        .collect();
    if draft_cards.is_empty() {
        return Ok(Report {
            picks: Vec::new(),
            benchmark: Benchmark::default(),
        });
    }
    let mut query = store
        .conn()
        .prepare("SELECT payload FROM tournament_records ORDER BY id LIMIT ?1")
        .map_err(|e| e.to_string())?;
    let payloads = query
        .query_map([MAX_RECORDS as i64], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let colors = draft.colors();
    let mut eligible = BTreeMap::new();
    let mut matched = Vec::new();
    let mut names = BTreeMap::new();
    let mut card_details = BTreeMap::new();
    let mut fingerprints = BTreeSet::new();
    for payload in payloads {
        let record: Record = serde_json::from_str(&payload.map_err(|e| e.to_string())?)
            .map_err(|e| format!("Stored tournament result: {e}"))?;
        validate(&record, now)?;
        let age = now - day(&record.date).ok_or("Invalid stored event date")?;
        if age > 365 {
            continue;
        }
        let main = resolve(store, &record.main, &mut names)?;
        let side = resolve(store, &record.sideboard, &mut names)?;
        // Exact repeated lists in one event are one observation, even if given different ids.
        if !fingerprints.insert((
            record.source_url.clone(),
            record.date.clone(),
            main.clone(),
            side.clone(),
        )) {
            continue;
        }
        let mut dot = 0f64;
        let mut norm = 0f64;
        for (id, n) in &main {
            let Some(card) = card_details
                .entry(*id)
                .or_insert_with(|| DraftCard::load(store, *id))
                .as_ref()
            else {
                continue;
            };
            if basic(card) {
                continue;
            }
            dot += f64::from(*n) * f64::from(draft_cards.get(id).copied().unwrap_or(0));
            norm += f64::from(*n).powi(2);
        }
        let draft_norm: f64 = draft_cards.values().map(|n| f64::from(*n).powi(2)).sum();
        let similarity = if norm > 0.0 {
            dot / (norm * draft_norm).sqrt()
        } else {
            0.0
        };
        if similarity < 0.2 {
            continue;
        }
        let weight = similarity
            * 2f64.powf(-(age as f64) / 90.0)
            * if record.placement.is_some_and(|p| p <= 8) {
                1.25
            } else {
                1.0
            };
        for id in main.keys().chain(side.keys()) {
            if eligible.contains_key(id) {
                continue;
            }
            let legal: bool = store
                .conn()
                .query_row(
                    "SELECT instr(legal_in, 'modern') > 0 FROM cards WHERE oracle=?1",
                    [id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            let identity = store.color_identity(*id).map_err(|e| e.to_string())?;
            eligible.insert(*id, legal && identity.chars().all(|c| colors.contains(c)));
        }
        matched.push((record, main, side, weight));
    }
    let samples = matched.len();
    let mut benchmark = Benchmark {
        samples,
        ..Default::default()
    };
    let mut mana_total = 0.0;
    let mut spell_total = 0.0;
    for (_, main, _, _) in &matched {
        for (id, count) in main {
            if let Some(card) = card_details.get(id).and_then(Option::as_ref) {
                if card.is_land() {
                    benchmark.lands += f64::from(*count) / samples as f64;
                } else {
                    benchmark.curve[(card.mana_value as usize).min(7)] +=
                        f64::from(*count) / samples as f64;
                    mana_total += f64::from(card.mana_value) * f64::from(*count);
                    spell_total += f64::from(*count);
                }
            }
        }
    }
    benchmark.average_mana = mana_total / spell_total.max(1.0);

    let total_weight: f64 = matched.iter().map(|(_, _, _, weight)| weight).sum();
    let mut picks = BTreeMap::<(u32, bool), Pick>::new();
    for (record, main, side, weight) in matched {
        for (sideboard, cards) in [(false, main), (true, side)] {
            for (id, n) in cards {
                if !eligible[&id] {
                    continue;
                }
                let Some(card) = card_details
                    .entry(id)
                    .or_insert_with(|| DraftCard::load(store, id))
                    .as_ref()
                    .filter(|card| !basic(card))
                else {
                    continue;
                };
                let pick = picks.entry((id, sideboard)).or_insert_with(|| Pick {
                    oracle: id,
                    name: card.name.clone(),
                    sideboard,
                    copies: 0,
                    included: 0,
                    samples,
                    first_date: record.date.clone(),
                    last_date: record.date.clone(),
                    sources: Vec::new(),
                    score: 0.0,
                });
                pick.included += 1;
                pick.copies += n;
                pick.first_date = pick.first_date.clone().min(record.date.clone());
                pick.last_date = pick.last_date.clone().max(record.date.clone());
                if pick.sources.len() < 3
                    && !pick
                        .sources
                        .iter()
                        .any(|(_, url)| *url == record.source_url)
                {
                    pick.sources
                        .push((record.event.clone(), record.source_url.clone()));
                }
                pick.score += weight;
            }
        }
    }
    let mut picks: Vec<_> = picks
        .into_values()
        .filter_map(|mut pick| {
            pick.copies = (pick.copies as f64 / pick.included as f64).round() as u32;
            let current = if pick.sideboard {
                &draft.side
            } else {
                &draft.main
            }
            .get(&pick.oracle)
            .copied()
            .unwrap_or(0);
            if current >= pick.copies {
                return None;
            }
            pick.score /= total_weight;
            Some(pick)
        })
        .collect();
    picks.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.sideboard.cmp(&b.sideboard))
    });
    picks.truncate(limit);
    Ok(Report { picks, benchmark })
}

#[cfg(test)]
fn recommend(store: &Store, draft: &Draft, now: i64, limit: usize) -> Result<Vec<Pick>, String> {
    Ok(report(store, draft, now, limit)?.picks)
}

pub(crate) const TEMPLATE: &str = r#"[
  {
    "id": "event-id/player-id",
    "event": "Event name",
    "date": "2026-10-01",
    "format": "modern",
    "source_url": "https://www.mtgo.com/decklists/",
    "placement": 8,
    "main": [{"name": "Replace with a card name", "count": 4}],
    "sideboard": []
  }
]"#;

#[derive(Default)]
pub(crate) struct ImportDetails {
    pub id: String,
    pub event: String,
    pub date: String,
    pub source_url: String,
    pub placement: String,
}

pub(crate) fn import_list(
    store: &Store,
    details: &ImportDetails,
    input: &str,
    now: i64,
) -> Result<usize, String> {
    if input.len() > MAX_BYTES {
        return Err("Tournament list exceeds 2 MiB.".into());
    }
    let parsed = mtg_deck::parse::parse(input);
    if !parsed.unparsed.is_empty() {
        return Err(format!(
            "Unparsed tournament lines: {}",
            parsed.unparsed.join("; ")
        ));
    }
    let mut main = Vec::new();
    let mut sideboard = Vec::new();
    for entry in parsed.entries {
        let card = NamedCard {
            name: entry.name,
            count: entry.count,
        };
        match entry.section {
            mtg_deck::parse::Section::Main => main.push(card),
            mtg_deck::parse::Section::Sideboard => sideboard.push(card),
            _ => {
                return Err(
                    "Modern tournament lists may contain only main-deck and sideboard entries."
                        .into(),
                );
            }
        }
    }
    let placement = if details.placement.trim().is_empty() {
        None
    } else {
        Some(
            details
                .placement
                .trim()
                .parse::<u32>()
                .map_err(|_| "Placement must be a positive integer.")?,
        )
    };
    let record = Record {
        id: details.id.trim().into(),
        event: details.event.trim().into(),
        date: details.date.trim().into(),
        format: "modern".into(),
        source_url: details.source_url.trim().into(),
        placement,
        main,
        sideboard,
    };
    import(
        store,
        &serde_json::to_string(&vec![record]).map_err(|e| e.to_string())?,
        now,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_store::{FaceRow, StoredCard};
    use serde_json::{Value, json};

    fn fixture() -> (Store, Draft) {
        let store = Store::in_memory().unwrap();
        let mut seed = 0;
        for (name, identity, legal, ty) in [
            ("Seed", "G", true, "Creature"),
            ("Recent", "G", true, "Artifact"),
            ("Old", "G", true, "Artifact"),
            ("Ancient", "G", true, "Artifact"),
            ("Other", "G", true, "Creature"),
            ("Blue", "U", true, "Instant"),
            ("Banned", "G", false, "Instant"),
            ("Side", "G", true, "Instant"),
            ("Forest", "G", true, "Basic Land — Forest"),
        ] {
            let id = store
                .put_card(
                    &StoredCard {
                        oracle_uuid: format!("fixture-{name}"),
                        name: name.into(),
                        layout: "normal".into(),
                        faces: vec![FaceRow {
                            name: name.into(),
                            type_line: ty.into(),
                            ..Default::default()
                        }],
                    },
                    &[],
                )
                .unwrap();
            store.set_color_identity(id, identity).unwrap();
            store
                .set_legal_in(id, if legal { &["modern"] } else { &[] })
                .unwrap();
            if name == "Seed" {
                seed = id;
            }
        }
        let mut draft = Draft::new("Tournament test");
        draft.add(&store, seed, false);
        (store, draft)
    }

    fn record(id: &str, date: &str, option: &str) -> Value {
        json!({"id": id, "event": format!("Event {id}"), "date": date,
            "format": "modern", "source_url": format!("https://example.com/{id}"), "placement": 8,
            "main": [{"name": "Seed", "count": 4}, {"name": option, "count": 4}, {"name":"Forest","count":20}],
            "sideboard": [{"name": "Side", "count": 2}]})
    }

    #[test]
    fn dates_validate_calendar_and_epoch_offsets() {
        assert_eq!(day("2024-03-01").unwrap() - day("2024-02-28").unwrap(), 2);
        assert!(day("2023-02-29").is_none());
        assert!(day("2026-04-31").is_none());
        assert!(day("2026-1-01").is_none());
        assert!(day("2026-é-01").is_none());
        assert_eq!(day("2000-01-01"), Some(10957));
    }

    #[test]
    fn imports_are_validated_atomic_and_idempotent() {
        let (store, _) = fixture();
        let now = day("2026-10-05").unwrap();
        let good = record("good", "2026-10-01", "Recent");
        let mut unknown = record("unknown", "2026-10-01", "Not in database");
        assert!(import(&store, &json!([good, unknown]).to_string(), now).is_err());
        assert_eq!(count(&store).unwrap(), 0);
        unknown["main"][1]["name"] = json!("Old");
        unknown["date"] = json!("2026-10-06");
        assert!(import(&store, &json!([unknown]).to_string(), now).is_err());
        assert_eq!(import(&store, &json!([good]).to_string(), now).unwrap(), 1);
        assert_eq!(import(&store, &json!([good]).to_string(), now).unwrap(), 1);
        assert_eq!(count(&store).unwrap(), 1);
        assert!(import(&store, &json!([good, good]).to_string(), now).is_err());
        store.conn().execute_batch("CREATE TRIGGER reject_result BEFORE INSERT ON tournament_records WHEN NEW.id = 'reject' BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        let batch = json!([
            record("new", "2026-10-01", "Recent"),
            record("reject", "2026-10-01", "Old")
        ]);
        assert!(import(&store, &batch.to_string(), now).is_err());
        assert_eq!(count(&store).unwrap(), 1);
    }

    #[test]
    fn modern_text_list_import_preserves_main_and_sideboard() {
        let (store, draft) = fixture();
        let now = day("2026-10-05").unwrap();
        let details = ImportDetails {
            id: "text".into(),
            event: "Text event".into(),
            date: "2026-10-01".into(),
            source_url: "https://example.com/text".into(),
            placement: "8".into(),
        };
        import_list(
            &store,
            &details,
            "4 Seed\n4 Recent\n20 Forest\n\nSideboard\n2 Side",
            now,
        )
        .unwrap();
        let picks = recommend(&store, &draft, now, 20).unwrap();
        assert!(
            picks
                .iter()
                .any(|p| p.name == "Recent" && !p.sideboard && p.copies == 4)
        );
        assert!(
            picks
                .iter()
                .any(|p| p.name == "Side" && p.sideboard && p.copies == 2)
        );
        assert!(import_list(&store, &details, "4 Unknown", now).is_err());
        assert_eq!(count(&store).unwrap(), 1);
    }

    #[test]
    fn recommendations_use_recency_similarity_legality_sections_and_deduplication() {
        let (store, mut draft) = fixture();
        let now = day("2026-10-05").unwrap();
        let recent = record("recent", "2026-10-01", "Recent");
        let mut duplicate = recent.clone();
        duplicate["id"] = json!("duplicate");
        let mut unrelated = record("unrelated", "2026-10-01", "Recent");
        unrelated["main"][0]["name"] = json!("Other");
        let records = json!([
            recent,
            duplicate,
            record("old", "2026-04-08", "Old"),
            record("stale", "2025-10-04", "Ancient"),
            unrelated,
            record("blue", "2026-10-01", "Blue"),
            record("banned", "2026-10-01", "Banned")
        ]);
        import(&store, &records.to_string(), now).unwrap();
        let picks = recommend(&store, &draft, now, 20).unwrap();
        let recent = picks.iter().find(|p| p.name == "Recent").unwrap();
        let old = picks.iter().find(|p| p.name == "Old").unwrap();
        assert!(recent.score > old.score);
        assert_eq!((recent.included, recent.samples), (1, 4));
        let benchmark = report(&store, &draft, now, 20).unwrap().benchmark;
        assert_eq!(benchmark.samples, 4);
        assert_eq!(benchmark.lands, 20.0);
        assert_eq!(benchmark.curve[0], 8.0);
        assert_eq!(benchmark.curve.iter().sum::<f64>(), 8.0);

        assert_eq!(recent.sources[0].1, "https://example.com/recent");
        assert_eq!(
            (&recent.first_date, &recent.last_date),
            (&"2026-10-01".to_owned(), &"2026-10-01".to_owned())
        );
        assert!(
            !picks
                .iter()
                .any(|p| ["Blue", "Banned", "Ancient", "Forest"].contains(&p.name.as_str()))
        );
        let seed = store.find_exact("Seed").unwrap().unwrap();
        draft.main.insert(seed, 4);
        let side = store.find_exact("Side").unwrap().unwrap();
        draft.add(&store, side, true);
        draft.side.insert(side, 2);
        let picks = recommend(&store, &draft, now, 20).unwrap();
        assert!(!picks.iter().any(|p| p.name == "Side" || p.name == "Seed"));
    }

    #[test]
    fn basic_lands_do_not_establish_an_archetype_match() {
        let (store, _) = fixture();
        let now = day("2026-10-05").unwrap();
        import(
            &store,
            &json!([record("one", "2026-10-01", "Recent")]).to_string(),
            now,
        )
        .unwrap();
        let mut draft = Draft::new("Only lands");
        draft.add(&store, store.find_exact("Forest").unwrap().unwrap(), false);
        assert!(recommend(&store, &draft, now, 20).unwrap().is_empty());
    }
}
