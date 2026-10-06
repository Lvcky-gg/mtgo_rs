//! Storing and querying the card database.

use rusqlite::{OptionalExtension, params};

use crate::{Result, Store};

/// A card as stored: printing-independent identity plus its faces.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredCard {
    /// Scryfall's `oracle_id`. The stable external identity.
    pub oracle_uuid: String,
    pub name: String,
    pub layout: String,
    pub faces: Vec<FaceRow>,
}

/// One face, with values as printed.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct FaceRow {
    pub name: String,
    pub mana_cost: String,
    pub type_line: String,
    pub oracle_text: Option<String>,
    /// Text, not a number: printed values include `*` and `1+*`.
    pub power: Option<String>,
    pub toughness: Option<String>,
    pub loyalty: Option<String>,
    /// Colors as single letters, e.g. `"WU"`.
    pub colors: String,
    /// Scryfall's CDN URL for this face's image. A pointer to fetch, never the image itself.
    pub image_uri: Option<String>,
    /// Scryfall's unique id for this specific printing variant (art). Enables selecting
    /// a specific artwork when adding cards to a deck.
    pub scryfall_id: Option<String>,
}

/// How a name was matched, so an import can report a loose match rather than hiding it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MatchKind {
    Exact,
    /// Matched on the normalised key — accents folded, punctuation dropped.
    Normalised,
    /// Matched one face of a multi-face card.
    FaceName,
}

impl Store {
    /// Atomically insert or replace a card, its faces, lookup keys, and search row.
    ///
    /// `keys` are the normalised lookup keys for each face, supplied by the caller
    /// because normalisation is a matching concern (`mtg-deck`) rather than a storage one.
    /// Returns the interned oracle id.
    pub fn put_card(&self, card: &StoredCard, keys: &[String]) -> Result<u32> {
        let tx = self.conn.unchecked_transaction()?;
        let conn = &tx;

        conn.execute(
            "INSERT INTO cards (oracle_uuid, name, layout) VALUES (?1, ?2, ?3)
             ON CONFLICT(oracle_uuid) DO UPDATE SET name = excluded.name, layout = excluded.layout",
            params![card.oracle_uuid, card.name, card.layout],
        )?;

        let oracle: u32 = conn.query_row(
            "SELECT oracle FROM cards WHERE oracle_uuid = ?1",
            params![card.oracle_uuid],
            |r| r.get(0),
        )?;

        // Replace rather than merge: a re-import should leave no trace of a previous
        // version of the card, or a face removed upstream would linger forever.
        conn.execute("DELETE FROM faces WHERE oracle = ?1", params![oracle])?;
        conn.execute("DELETE FROM face_keys WHERE oracle = ?1", params![oracle])?;

        for (i, face) in card.faces.iter().enumerate() {
            conn.execute(
                "INSERT INTO faces
                   (oracle, face_index, name, mana_cost, type_line, oracle_text,
                    power, toughness, loyalty, colors, image_uri, scryfall_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                    face.scryfall_id,
                ],
            )?;
            if let Some(key) = keys.get(i) {
                conn.execute(
                    "INSERT INTO face_keys (oracle, face_index, key) VALUES (?1, ?2, ?3)",
                    params![oracle, i as i64, key],
                )?;
            }
        }

        // Search rows use the stable card id as their indexed FTS rowid.
        conn.execute("DELETE FROM card_search WHERE rowid = ?1", params![oracle])?;
        let front = card.faces.first();
        conn.execute(
            "INSERT INTO card_search (name, type_line, oracle_text, oracle, rowid)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![
                card.name,
                front.map(|f| f.type_line.clone()).unwrap_or_default(),
                front
                    .and_then(|f| f.oracle_text.clone())
                    .unwrap_or_default(),
                oracle,
            ],
        )?;

        tx.commit()?;
        Ok(oracle)
    }

    /// How many cards are stored.
    pub fn card_count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM cards", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Load one card by interned id.
    pub fn card(&self, oracle: u32) -> Result<Option<StoredCard>> {
        let head = self
            .conn
            .query_row(
                "SELECT oracle_uuid, name, layout FROM cards WHERE oracle = ?1",
                params![oracle],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((oracle_uuid, name, layout)) = head else {
            return Ok(None);
        };

        Ok(Some(StoredCard {
            oracle_uuid,
            name,
            layout,
            faces: self.faces(oracle)?,
        }))
    }

    fn faces(&self, oracle: u32) -> Result<Vec<FaceRow>> {
        let mut q = self.conn.prepare(
            "SELECT name, mana_cost, type_line, oracle_text, power, toughness, loyalty, colors,
                    image_uri, scryfall_id
             FROM faces WHERE oracle = ?1 ORDER BY face_index",
        )?;
        let rows = q.query_map(params![oracle], |r| {
            Ok(FaceRow {
                name: r.get(0)?,
                mana_cost: r.get(1)?,
                type_line: r.get(2)?,
                oracle_text: r.get(3)?,
                power: r.get(4)?,
                toughness: r.get(5)?,
                loyalty: r.get(6)?,
                colors: r.get(7)?,
                image_uri: r.get(8)?,
                scryfall_id: r.get(9)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Every card, in id order. Used to build the in-memory library the engine plays from.
    pub fn all_cards(&self) -> Result<Vec<(u32, StoredCard)>> {
        let mut q = self
            .conn
            .prepare("SELECT oracle, oracle_uuid, name, layout FROM cards ORDER BY oracle")?;
        let heads: Vec<(u32, String, String, String)> = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;

        let mut out = Vec::with_capacity(heads.len());
        for (oracle, oracle_uuid, name, layout) in heads {
            out.push((
                oracle,
                StoredCard {
                    oracle_uuid,
                    name,
                    layout,
                    faces: self.faces(oracle)?,
                },
            ));
        }
        Ok(out)
    }

    // ---- name resolution, for deck import -----------------------------

    /// Exact match on a card's full printed name.
    pub fn find_exact(&self, name: &str) -> Result<Option<u32>> {
        Ok(self
            .conn
            .query_row(
                "SELECT oracle FROM cards WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Match on a normalised key.
    pub fn find_normalised(&self, key: &str) -> Result<Option<u32>> {
        Ok(self
            .conn
            .query_row(
                "SELECT oracle FROM face_keys WHERE key = ?1 ORDER BY face_index LIMIT 1",
                params![key],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Match a name that is only one face of a multi-face card.
    pub fn find_by_face_name(&self, name: &str) -> Result<Option<u32>> {
        Ok(self
            .conn
            .query_row(
                "SELECT oracle FROM faces WHERE name = ?1 ORDER BY face_index LIMIT 1",
                params![name],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The interned id for a Scryfall oracle id. How a deck sent by another installation —
    /// whose own interned ids mean nothing here — is resolved.
    pub fn find_by_uuid(&self, oracle_uuid: &str) -> Result<Option<u32>> {
        Ok(self
            .conn
            .query_row(
                "SELECT oracle FROM cards WHERE oracle_uuid = ?1",
                params![oracle_uuid],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// A card's colour identity as letters (`"WU"`), empty for colourless.
    pub fn color_identity(&self, oracle: u32) -> Result<String> {
        Ok(self
            .conn
            .query_row(
                "SELECT color_identity FROM cards WHERE oracle = ?1",
                params![oracle],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_default())
    }

    /// Record where a card is legal: the format names, e.g. `["standard", "modern"]`.
    pub fn set_legal_in(&self, oracle: u32, formats: &[&str]) -> Result<()> {
        let joined = if formats.is_empty() {
            String::new()
        } else {
            format!(",{},", formats.join(","))
        };
        self.conn.execute(
            "UPDATE cards SET legal_in = ?1 WHERE oracle = ?2",
            params![joined, oracle],
        )?;
        Ok(())
    }

    /// Record a card's colour identity.
    pub fn set_color_identity(&self, oracle: u32, identity: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE cards SET color_identity = ?1 WHERE oracle = ?2",
            params![identity, oracle],
        )?;
        Ok(())
    }

    /// The image URL for a card or face name, if the import recorded one.
    ///
    /// A face name wins over a card name, so the back of a double-faced card shows its own
    /// image; a full name like `"A // B"` falls back to the front face.
    pub fn image_uri(&self, name: &str) -> Result<Option<String>> {
        let by_face = self
            .conn
            .query_row(
                "SELECT image_uri FROM faces WHERE name = ?1 AND image_uri IS NOT NULL
                 ORDER BY face_index LIMIT 1",
                params![name],
                |r| r.get(0),
            )
            .optional()?;
        if by_face.is_some() {
            return Ok(by_face);
        }
        Ok(self
            .conn
            .query_row(
                "SELECT f.image_uri FROM cards c JOIN faces f ON f.oracle = c.oracle
                 WHERE c.name = ?1 AND f.image_uri IS NOT NULL
                 ORDER BY f.face_index LIMIT 1",
                params![name],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Near matches, for a "did you mean" list on a failed import.
    ///
    /// A prefix search rather than an edit-distance one: it is what actually helps with
    /// a truncated or half-remembered name, and it is an index hit rather than a scan.
    pub fn suggest(&self, name: &str, limit: usize) -> Result<Vec<(u32, String)>> {
        let mut q = self.conn.prepare(
            "SELECT oracle, name FROM cards WHERE name LIKE ?1 ORDER BY LENGTH(name) LIMIT ?2",
        )?;
        // `%` and `_` are LIKE wildcards; a name containing them would otherwise match
        // far more than the user typed.
        let escaped: String = name.chars().filter(|c| *c != '%' && *c != '_').collect();
        let pattern = format!("{escaped}%");
        let rows = q.query_map(params![pattern, limit as i64], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Browse cards for deck building: optional words (full-text over name, type and rules
    /// text), a type-line fragment, and a colour restriction. Sorted by name, a page at a time.
    pub fn browse(&self, q: &CardQuery, limit: usize, offset: usize) -> Result<Vec<(u32, String)>> {
        let mut sql = String::from(
            "SELECT c.oracle, c.name FROM cards c
             JOIN faces f ON f.oracle = c.oracle AND f.face_index = 0 WHERE 1 = 1",
        );
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        let words = fts_query(&q.text);
        if !words.is_empty() {
            sql.push_str(
                " AND c.oracle IN (SELECT oracle FROM card_search WHERE card_search MATCH ?)",
            );
            args.push(Box::new(words));
        }
        if !q.type_contains.trim().is_empty() {
            sql.push_str(" AND f.type_line LIKE ?");
            args.push(Box::new(format!("%{}%", q.type_contains.trim())));
        }
        match q.legal_in.as_deref() {
            None => {}
            // Legal somewhere: leaves out joke, digital-only and playtest cards.
            Some("") => sql.push_str(" AND c.legal_in != ''"),
            Some(format) => {
                sql.push_str(" AND instr(c.legal_in, ?) > 0");
                args.push(Box::new(format!(",{format},")));
            }
        }
        if let Some(allowed) = &q.colors_within {
            // Every colour of the card's identity must be one of the allowed ones.
            for letter in ["W", "U", "B", "R", "G"] {
                if !allowed.contains(letter) {
                    sql.push_str(" AND instr(c.color_identity, ?) = 0");
                    args.push(Box::new(letter.to_string()));
                }
            }
        }
        // With words to find, the card *named* that comes first, then names starting with it,
        // then names containing it, and only then cards that merely mention it — "forest" should
        // find Forest before every card whose text says "Forest".
        let phrase = q.text.trim().to_lowercase();
        if phrase.is_empty() {
            sql.push_str(" ORDER BY c.name");
        } else {
            sql.push_str(
                " ORDER BY CASE WHEN lower(c.name) = ? THEN 0 WHEN lower(c.name) LIKE ? THEN 1 \
                 WHEN lower(c.name) LIKE ? THEN 2 ELSE 3 END, c.name",
            );
            args.push(Box::new(phrase.clone()));
            args.push(Box::new(format!("{phrase}%")));
            args.push(Box::new(format!("%{phrase}%")));
        }
        sql.push_str(" LIMIT ? OFFSET ?");
        args.push(Box::new(limit as i64));
        args.push(Box::new(offset as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
        let rows = stmt.query_map(params.as_slice(), |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Full-text search over name, type line and rules text.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(u32, String)>> {
        // FTS5 treats a lot of punctuation as syntax, so a user's raw words are quoted
        // rather than passed through as a query expression.
        let safe = query
            .split_whitespace()
            .map(|w| format!("\"{}\"", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" ");
        if safe.is_empty() {
            return Ok(Vec::new());
        }

        let mut q = self.conn.prepare(
            "SELECT cs.oracle, c.name
             FROM card_search cs JOIN cards c ON c.oracle = cs.oracle
             WHERE card_search MATCH ?1
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = q.query_map(params![safe, limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // ---- subtype interning ---------------------------------------------

    /// Intern a subtype name, returning its stable id.
    pub fn intern_subtype(&self, name: &str) -> Result<u16> {
        if let Some(id) = self
            .conn
            .query_row(
                "SELECT id FROM subtypes WHERE name = ?1",
                params![name],
                |r| r.get::<_, u16>(0),
            )
            .optional()?
        {
            return Ok(id);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO subtypes (name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
            params![name],
        )?;
        let id = tx.query_row(
            "SELECT id FROM subtypes WHERE name = ?1",
            params![name],
            |r| r.get::<_, u16>(0),
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// Every interned subtype, for building the engine's lookup table.
    pub fn subtypes(&self) -> Result<Vec<(u16, String)>> {
        let mut q = self
            .conn
            .prepare("SELECT id, name FROM subtypes ORDER BY id")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, u16>(0)?, r.get(1)?)))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

/// What to browse for. Empty fields do not filter.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct CardQuery {
    /// Words to find in the name, type line or rules text.
    pub text: String,
    /// A fragment of the type line: "Creature", "Elf", "Legendary".
    pub type_contains: String,
    /// Colour letters the card's identity must stay within, e.g. "WU". `None` for any.
    pub colors_within: Option<String>,
    /// Only cards legal in this format (Scryfall's name: "modern", "commander"). `Some("")` for
    /// legal in any format; `None` for everything, legal or not.
    pub legal_in: Option<String>,
}

/// A user's words as a safe FTS5 query: each word quoted and prefix-matched, so punctuation
/// is never read as query syntax and "light bol" finds Lightning Bolt.
fn fts_query(text: &str) -> String {
    text.split_whitespace()
        .map(|w| w.replace('"', ""))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect::<Vec<_>>()
        .join(" ")
}
