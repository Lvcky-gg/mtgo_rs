//! Streaming a bulk export into the local database.
//!
//! The bulk file is a single JSON array of a few hundred megabytes, so it is read
//! **element by element** rather than parsed into memory first. Serde supports this
//! through a seeded deserializer: the visitor is handed a `SeqAccess` and pulls one card
//! at a time, so peak memory is one card regardless of file size.
//!
//! Unsupported card rows are reported, not fatal. A bulk file spanning the game's whole
//! history contains rows this client does not understand, and an import that aborted on the first one would
//! never finish. Everything unhandled is counted in [`ImportReport`], which is the point:
//! the report is how the gap between "card exists" and "card is playable" stays visible.
//! Database failures abort and roll back the import, preserving the previous data.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Cursor, Read};

use mtg_store::Store;
use serde::de::{DeserializeSeed, SeqAccess, Visitor};

use crate::{convert, manacost, scryfall, typeline};

const MAX_GZIP_LAYERS: usize = 4;

/// What an import did, and what it could not do.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct ImportReport {
    /// Cards written.
    pub imported: u64,
    /// Rows deliberately not imported, by reason.
    pub skipped: BTreeMap<&'static str, u64>,
    /// Cards whose mana cost contained a symbol this build does not know.
    pub unparsed_costs: u64,
    /// Cards whose type line contained a word this build does not know.
    pub unknown_type_words: u64,
    /// Rows that could not be read or converted. Database write failures abort the import.
    pub failed: u64,
    /// Distinct subtypes interned.
    pub subtypes: u64,
}

impl ImportReport {
    fn skip(&mut self, reason: scryfall::Skip) {
        let key = match reason {
            scryfall::Skip::NoOracleId => "not a card",
            scryfall::Skip::NotEnglish => "not English",
            scryfall::Skip::NotPlayable => "not a playable layout",
        };
        *self.skipped.entry(key).or_insert(0) += 1;
    }

    /// A one-line summary for a CLI.
    pub fn summary(&self) -> String {
        let skipped: u64 = self.skipped.values().sum();
        format!(
            "{} cards imported, {} skipped, {} subtypes; \
             {} costs and {} type lines not fully understood, {} failed",
            self.imported,
            skipped,
            self.subtypes,
            self.unparsed_costs,
            self.unknown_type_words,
            self.failed
        )
    }
}

/// Progress from a streaming import. Counts include skipped and unreadable rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImportProgress {
    Reading { processed: u64, imported: u64 },
    Committing,
}

impl ImportProgress {
    pub fn status(&self) -> String {
        match self {
            Self::Reading {
                processed,
                imported,
            } => format!("Importing cards: {imported} imported, {processed} read"),
            Self::Committing => "Saving card database…".into(),
        }
    }
}

/// Import a bulk export from any reader.
///
/// Takes a reader rather than a URL so the whole pipeline is testable with a few bytes and
/// no network, and so a user can import a file they already have.
///
/// The format is detected, not assumed, because Scryfall has shipped more than one: a single
/// JSON array (the original bulk files), and gzip-compressed JSON Lines — one card per line —
/// which replaced it in 2026. Gzip is recognised by its magic bytes, then the first
/// non-whitespace byte tells an array (`[`) from lines (`{`).
pub fn import<R: Read>(store: &mut Store, reader: R) -> Result<ImportReport, String> {
    import_with_progress(store, reader, &mut |_| {})
}

/// Import with inexpensive progress notifications every 500 rows and at completion.
pub fn import_with_progress<R: Read>(
    store: &mut Store,
    reader: R,
    progress: &mut dyn FnMut(ImportProgress),
) -> Result<ImportReport, String> {
    import_with_timestamp(store, reader, progress, None)
}

/// File/download wrappers commit their refresh timestamp alongside the imported data.
pub(crate) fn import_with_timestamp<R: Read>(
    store: &mut Store,
    reader: R,
    progress: &mut dyn FnMut(ImportProgress),
    timestamp: Option<u64>,
) -> Result<ImportReport, String> {
    progress(ImportProgress::Reading {
        processed: 0,
        imported: 0,
    });
    import_boxed(store, Box::new(reader), progress, timestamp, 0)
}

/// The body of [`import`], taking a boxed reader so that unwrapping gzip can recurse without
/// the reader's type growing with each level.
fn import_boxed(
    store: &mut Store,
    mut reader: Box<dyn Read + '_>,
    progress: &mut dyn FnMut(ImportProgress),
    timestamp: Option<u64>,
    gzip_layers: usize,
) -> Result<ImportReport, String> {
    // Read may return only one byte even when more data is coming. Preserve the
    // prefix so both gzip decoding and JSON parsing still consume the whole input.
    let mut prefix = Vec::with_capacity(2);
    reader
        .by_ref()
        .take(2)
        .read_to_end(&mut prefix)
        .map_err(|e| e.to_string())?;
    let compressed = prefix == [0x1f, 0x8b];
    let reader = Cursor::new(prefix).chain(reader);
    if compressed {
        if gzip_layers >= MAX_GZIP_LAYERS {
            return Err("too many nested gzip layers in the card export".into());
        }
        return import_boxed(
            store,
            Box::new(flate2::read::MultiGzDecoder::new(reader)),
            progress,
            timestamp,
            gzip_layers + 1,
        );
    }
    let mut reader = BufReader::new(reader);
    let first = loop {
        let buf = reader.fill_buf().map_err(|e| e.to_string())?;
        match buf.iter().position(|b| !b.is_ascii_whitespace()) {
            Some(i) => break Some(buf[i]),
            None if buf.is_empty() => break None,
            None => {
                let n = buf.len();
                reader.consume(n);
            }
        }
    };

    let mut report = ImportReport::default();

    // One transaction for the whole import: a half-imported card database is worse than
    // none, and committing per card would make the write cost dominate.
    let tx = store.conn_mut().transaction().map_err(|e| e.to_string())?;
    {
        let mut sink = Sink {
            tx: &tx,
            report: &mut report,
            seen_subtypes: Default::default(),
            processed: 0,
            progress,
        };
        match first {
            Some(b'[') => {
                let mut de = serde_json::Deserializer::from_reader(reader);
                sink.deserialize(&mut de).map_err(|e| e.to_string())?;
                de.end().map_err(|e| e.to_string())?;
            }
            Some(b'{') => {
                // JSON Lines: stream line by line, so peak memory is still one card. A line
                // that does not parse is counted and skipped, like any other unreadable row.
                for line in reader.lines() {
                    let line = line.map_err(|e| e.to_string())?;
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<scryfall::Card>(&line) {
                        Ok(card) => match card.importable() {
                            Ok(()) => sink.write(&card).map_err(|e| e.to_string())?,
                            Err(reason) => sink.report.skip(reason),
                        },
                        Err(_) => sink.report.failed += 1,
                    }
                    sink.row_processed();
                }
            }
            Some(other) => {
                return Err(format!(
                    "not a card export: it starts with {:?}, not a JSON array or JSON lines",
                    other as char
                ));
            }
            None => return Err("the file is empty".into()),
        }
    }
    progress(ImportProgress::Reading {
        processed: report.imported + report.failed + report.skipped.values().sum::<u64>(),
        imported: report.imported,
    });
    progress(ImportProgress::Committing);
    tx.execute(
        "INSERT INTO meta (key, value) VALUES ('cards_imported', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [report.imported.to_string()],
    )
    .map_err(|e| e.to_string())?;
    if let Some(timestamp) = timestamp {
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('last_import_unix', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [timestamp.to_string()],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(report)
}

/// The streaming visitor: pulls one card at a time out of the array.
struct Sink<'a> {
    tx: &'a rusqlite_shim::Transaction<'a>,
    report: &'a mut ImportReport,
    seen_subtypes: std::collections::BTreeSet<String>,
    processed: u64,
    progress: &'a mut dyn FnMut(ImportProgress),
}

/// Re-exported so this module does not need `rusqlite` as a direct dependency just to name
/// a transaction type.
mod rusqlite_shim {
    pub use mtg_store::rusqlite::Transaction;
}

impl<'de> DeserializeSeed<'de> for Sink<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for Sink<'_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a JSON array of Scryfall card objects")
    }

    fn visit_seq<A>(mut self, mut seq: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        while let Some(card) = seq.next_element::<scryfall::Card>()? {
            if let Err(reason) = card.importable() {
                self.report.skip(reason);
                self.row_processed();
                continue;
            }
            self.write(&card).map_err(serde::de::Error::custom)?;
            self.row_processed();
        }
        Ok(())
    }
}

impl Sink<'_> {
    fn row_processed(&mut self) {
        self.processed += 1;
        if self.processed.is_multiple_of(500) {
            (self.progress)(ImportProgress::Reading {
                processed: self.processed,
                imported: self.report.imported,
            });
        }
    }

    fn write(&mut self, card: &scryfall::Card) -> Result<(), mtg_store::rusqlite::Error> {
        let Some(stored) = convert::to_stored(card) else {
            self.report.failed += 1;
            return Ok(());
        };

        // Note what this build did not fully understand, per card rather than per symbol,
        // so the numbers read as "cards needing attention".
        let mut cost_gap = false;
        let mut type_gap = false;
        for face in &stored.faces {
            if !manacost::parse(&face.mana_cost).1.symbols.is_empty() {
                cost_gap = true;
            }
            let parsed = typeline::parse(&face.type_line);
            if !parsed.unknown.is_empty() {
                type_gap = true;
            }
            for sub in &parsed.subtypes {
                if self.seen_subtypes.insert(sub.clone()) {
                    intern_subtype(self.tx, sub)?;
                    self.report.subtypes += 1;
                }
            }
        }
        if cost_gap {
            self.report.unparsed_costs += 1;
        }
        if type_gap {
            self.report.unknown_type_words += 1;
        }

        let keys: Vec<String> = stored
            .faces
            .iter()
            .map(|f| mtg_deck::resolve::normalise(&f.name))
            .collect();

        let identity = card.color_identity.clone().unwrap_or_default().join("");
        let legal: Vec<&str> = card
            .legalities
            .iter()
            .flatten()
            .filter(|(_, status)| matches!(status.as_str(), "legal" | "restricted"))
            .map(|(format, _)| format.as_str())
            .collect();
        let legal_in = if legal.is_empty() {
            String::new()
        } else {
            format!(",{},", legal.join(","))
        };
        put_card(self.tx, &stored, &keys)?;
        self.tx.execute(
            "UPDATE cards SET color_identity = ?1, legal_in = ?2 WHERE oracle_uuid = ?3",
            mtg_store::rusqlite::params![identity, legal_in, stored.oracle_uuid],
        )?;
        self.report.imported += 1;
        Ok(())
    }
}

// The store's own methods take `&Store`, which owns its connection; an import runs inside
// one transaction, so these mirror them against a `Transaction` instead. Kept next to the
// importer rather than in the store because the transaction-scoped form exists only for
// this one caller.

fn intern_subtype(
    tx: &rusqlite_shim::Transaction<'_>,
    name: &str,
) -> Result<(), mtg_store::rusqlite::Error> {
    tx.execute(
        "INSERT INTO subtypes (name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
        [name],
    )?;
    tx.query_row("SELECT id FROM subtypes WHERE name = ?1", [name], |row| {
        row.get::<_, u16>(0)
    })?;
    Ok(())
}

fn put_card(
    tx: &rusqlite_shim::Transaction<'_>,
    card: &mtg_store::StoredCard,
    keys: &[String],
) -> Result<(), mtg_store::rusqlite::Error> {
    use mtg_store::rusqlite::params;

    tx.execute(
        "INSERT INTO cards (oracle_uuid, name, layout) VALUES (?1, ?2, ?3)
         ON CONFLICT(oracle_uuid) DO UPDATE SET name = excluded.name, layout = excluded.layout",
        params![card.oracle_uuid, card.name, card.layout],
    )?;
    let oracle: i64 = tx.query_row(
        "SELECT oracle FROM cards WHERE oracle_uuid = ?1",
        params![card.oracle_uuid],
        |r| r.get(0),
    )?;

    tx.execute("DELETE FROM faces WHERE oracle = ?1", params![oracle])?;
    tx.execute("DELETE FROM face_keys WHERE oracle = ?1", params![oracle])?;

    for (i, face) in card.faces.iter().enumerate() {
        tx.execute(
            "INSERT INTO faces
               (oracle, face_index, name, mana_cost, type_line, oracle_text,
                power, toughness, loyalty, colors, image_uri)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                oracle,
                i as i64,
                face.name,
                face.mana_cost,
                face.type_line,
                face.oracle_text,
                face.power,
                face.toughness,
                face.loyalty,
                face.colors,
                face.image_uri,
            ],
        )?;
        if let Some(key) = keys.get(i) {
            tx.execute(
                "INSERT INTO face_keys (oracle, face_index, key) VALUES (?1, ?2, ?3)",
                params![oracle, i as i64, key],
            )?;
        }
    }

    tx.execute("DELETE FROM card_search WHERE rowid = ?1", params![oracle])?;
    let front = card.faces.first();
    tx.execute(
        "INSERT INTO card_search (name, type_line, oracle_text, oracle, rowid) VALUES (?1, ?2, ?3, ?4, ?4)",
        params![
            card.name,
            front.map(|f| f.type_line.clone()).unwrap_or_default(),
            front
                .and_then(|f| f.oracle_text.clone())
                .unwrap_or_default(),
            oracle,
        ],
    )?;

    Ok(())
}
