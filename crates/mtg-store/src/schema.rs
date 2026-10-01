//! Schema and migrations.
//!
//! Migrations are a numbered list applied in order, with the reached version recorded in
//! `meta`. Deliberately plain: a migration framework would be a dependency bought to
//! manage a handful of statements, and the card IR is going to churn, so what matters is
//! that adding a step is trivial and that an older client refuses a newer file rather
//! than corrupting it.

use crate::{Result, Store, StoreError};

/// The schema version this build writes and understands.
pub const VERSION: u32 = 8;

const MIGRATIONS: &[&str] = &[
    // v1 — cards, decks, preferences.
    r#"
    -- One row per oracle identity: the printing-independent card.
    CREATE TABLE cards (
        oracle       INTEGER PRIMARY KEY,
        oracle_uuid  TEXT    NOT NULL UNIQUE,
        name         TEXT    NOT NULL,
        layout       TEXT    NOT NULL
    );
    CREATE INDEX cards_name ON cards (name);

    -- Split, adventure and double-faced cards have more than one face. Index 0 is the
    -- front. Power and toughness are TEXT because printed values include '*' and '1+*'.
    CREATE TABLE faces (
        oracle      INTEGER NOT NULL REFERENCES cards(oracle) ON DELETE CASCADE,
        face_index  INTEGER NOT NULL,
        name        TEXT    NOT NULL,
        mana_cost   TEXT    NOT NULL DEFAULT '',
        type_line   TEXT    NOT NULL DEFAULT '',
        oracle_text TEXT,
        power       TEXT,
        toughness   TEXT,
        loyalty     TEXT,
        colors      TEXT    NOT NULL DEFAULT '',
        PRIMARY KEY (oracle, face_index)
    );
    CREATE INDEX faces_name ON faces (name);

    -- Subtypes are interned so the engine can compare them as integers.
    CREATE TABLE subtypes (
        id   INTEGER PRIMARY KEY,
        name TEXT    NOT NULL UNIQUE
    );

    -- A normalised key per face, for import name resolution: lowercased, accents folded,
    -- punctuation dropped. Kept as a column rather than computed per query so the lookup
    -- is an index hit.
    CREATE TABLE face_keys (
        oracle     INTEGER NOT NULL REFERENCES cards(oracle) ON DELETE CASCADE,
        face_index INTEGER NOT NULL,
        key        TEXT    NOT NULL
    );
    CREATE INDEX face_keys_key ON face_keys (key);

    -- Full-text search over what a deck builder actually searches.
    CREATE VIRTUAL TABLE card_search USING fts5 (
        name,
        type_line,
        oracle_text,
        oracle UNINDEXED
    );

    CREATE TABLE decks (
        id      INTEGER PRIMARY KEY,
        name    TEXT    NOT NULL,
        format  TEXT,
        created TEXT    NOT NULL
    );

    CREATE TABLE deck_entries (
        deck    INTEGER NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
        oracle  INTEGER NOT NULL,
        count   INTEGER NOT NULL,
        section TEXT    NOT NULL
    );
    CREATE INDEX deck_entries_deck ON deck_entries (deck);

    CREATE TABLE prefs (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    "#,
    // v2 — identity and pinned peers.
    r#"
    -- Exactly one row: this installation's keypair. The public key *is* the account, so
    -- losing this row loses the identity — nothing else has a copy.
    CREATE TABLE identity (
        id      INTEGER PRIMARY KEY CHECK (id = 1),
        seed    BLOB    NOT NULL,
        public  BLOB    NOT NULL,
        created TEXT    NOT NULL
    );

    -- Peers trusted on first use. There is no directory to disagree with, so the nickname
    -- is local and the key is the identity.
    CREATE TABLE friends (
        public   BLOB PRIMARY KEY,
        nickname TEXT NOT NULL,
        first_seen TEXT NOT NULL
    );
    "#,
    // v3 — card images.
    r#"
    -- Where a face's image lives on Scryfall's CDN. A URL, not the image: images are cached
    -- on disk by the client, never stored here. NULL until a re-import supplies it.
    ALTER TABLE faces ADD COLUMN image_uri TEXT;
    "#,
    // v4 — colour identity, for Commander deck building.
    r#"
    -- Letters, e.g. 'WU'. Empty for colourless, and until a re-import supplies it.
    ALTER TABLE cards ADD COLUMN color_identity TEXT NOT NULL DEFAULT '';
    "#,
    // v5 — format legality, for filtering what the deck builder offers.
    r#"
    -- Comma-separated formats where the card is legal or restricted: ',standard,modern,'.
    -- Leading and trailing commas make "is it legal in X" a plain substring test. Empty
    -- means not legal anywhere: joke cards, digital-only rebalances, playtest cards.
    ALTER TABLE cards ADD COLUMN legal_in TEXT NOT NULL DEFAULT '';
    -- The column is only filled by an import, so the next download must not be skipped as
    -- "already done today".
    DELETE FROM meta WHERE key = 'last_import_unix';
    "#,
    // v6 — import updates address one card instead of scanning the full index.
    r#"
    CREATE INDEX face_keys_oracle ON face_keys (oracle);
    -- FTS's ordinary oracle column is UNINDEXED. Its rowid is indexed, so bind
    -- each search document to the stable card id and rebuild existing documents.
    DELETE FROM card_search;
    INSERT INTO card_search (rowid, name, type_line, oracle_text, oracle)
        SELECT c.oracle, c.name, COALESCE(f.type_line, ''),
               COALESCE(f.oracle_text, ''), c.oracle
        FROM cards c LEFT JOIN faces f ON f.oracle = c.oracle AND f.face_index = 0;
    "#,
    // v7 — card printing selection: track scryfall_id per face for artwork variants.
    r#"
    -- Scryfall's unique id for this specific printing. NULL for cards from before the
    -- migration or custom demo cards. Multiple faces can share a scryfall_id if they're
    -- from the same card (e.g., transform cards).
    ALTER TABLE faces ADD COLUMN scryfall_id TEXT;
    -- Create index for efficient lookup when selecting a specific printing.
    CREATE INDEX faces_scryfall_id ON faces (scryfall_id);
    "#,
    // v8 — artwork preferences belong to a deck, independent of oracle rules.
    r#"
    CREATE TABLE deck_printings (
        deck INTEGER NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
        oracle INTEGER NOT NULL,
        printing TEXT NOT NULL,
        PRIMARY KEY (deck, oracle)
    );
    "#,
];

/// Apply any migrations the file has not seen.
pub fn migrate(store: &mut Store) -> Result<()> {
    // `meta` does not exist before the first migration, so the version is read from
    // `user_version`, which SQLite maintains for exactly this purpose.
    let current: u32 = store
        .conn
        .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?
        as u32;

    if current > VERSION {
        return Err(StoreError::SchemaTooNew {
            found: current,
            supported: VERSION,
        });
    }

    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let version = i as u32 + 1;
        if version <= current {
            continue;
        }
        let tx = store.conn.transaction()?;
        tx.execute_batch(sql)?;
        // `user_version` does not accept a bound parameter.
        tx.pragma_update(None, "user_version", version as i64)?;
        tx.commit()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_six_rebuilds_old_search_rowids_without_changing_cards_or_decks() {
        let mut store = Store::in_memory().unwrap();
        // Drop the v7 faces table and recreate it as v5 (without scryfall_id column)
        store.conn.execute_batch(r#"
            DROP TABLE IF EXISTS faces;
            DROP TABLE IF EXISTS deck_printings;
            CREATE TABLE faces (
                oracle      INTEGER NOT NULL REFERENCES cards(oracle) ON DELETE CASCADE,
                face_index  INTEGER NOT NULL,
                name        TEXT    NOT NULL,
                mana_cost   TEXT    NOT NULL DEFAULT '',
                type_line   TEXT    NOT NULL DEFAULT '',
                oracle_text TEXT,
                power       TEXT,
                toughness   TEXT,
                loyalty     TEXT,
                colors      TEXT    NOT NULL DEFAULT '',
                image_uri   TEXT,
                PRIMARY KEY (oracle, face_index)
            );
            CREATE INDEX faces_name ON faces (name);
            INSERT INTO cards (oracle, oracle_uuid, name, layout) VALUES (42, 'invented', 'Maple Scout', 'normal');
            INSERT INTO faces (oracle, face_index, name, type_line, oracle_text)
                VALUES (42, 0, 'Maple Scout', 'Creature', 'Reach');
            INSERT INTO decks (id, name, created) VALUES (7, 'My deck', 'today');
            INSERT INTO deck_entries (deck, oracle, count, section) VALUES (7, 42, 4, 'main');
            INSERT INTO card_search (rowid, name, oracle) VALUES (900, 'Obsolete Search', 42), (901, 'Duplicate Search', 42);
            DROP INDEX face_keys_oracle;
            PRAGMA user_version = 5;
        "#).unwrap();
        migrate(&mut store).unwrap();
        let docs: Vec<(i64, String, i64)> = store
            .conn
            .prepare("SELECT rowid, name, oracle FROM card_search")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(docs, vec![(42, "Maple Scout".into(), 42)]);
        let found: i64 = store
            .conn
            .query_row(
                "SELECT oracle FROM card_search WHERE card_search MATCH 'Reach'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(found, 42);
        let copies: i64 = store
            .conn
            .query_row("SELECT count FROM deck_entries WHERE deck = 7", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(copies, 4);
        let plan: String = store
            .conn
            .query_row(
                "EXPLAIN QUERY PLAN DELETE FROM face_keys WHERE oracle = 42",
                [],
                |r| r.get(3),
            )
            .unwrap();
        assert!(plan.contains("face_keys_oracle"), "{plan}");
        migrate(&mut store).unwrap();
        let count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM card_search", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
