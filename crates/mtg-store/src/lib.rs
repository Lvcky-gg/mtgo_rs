//! The local database.
//!
//! One SQLite file holds everything the client keeps: the imported card database, decks,
//! preferences, and (later) match history and replays. One store rather than several
//! because a deck import and the match that used it should be able to commit together,
//! and because "back up your collection" should mean copying one file.
//!
//! # What lives here and what does not
//!
//! This crate owns *persistence*. It does not know what a card means — `mtg-ir` defines
//! that — and it does not know where cards come from, which is `mtg-oracle`'s job. It
//! stores what it is given and answers queries about it.
//!
//! Card text is stored as Scryfall gives it: `power` and `toughness` are **text**, not
//! integers, because printed values include `*` and `1+*`. Interpreting them is a
//! parsing concern, not a storage one, and coercing on the way in would lose information
//! the engine may later need.
//!
//! # Ships no card data
//!
//! The schema is here; the rows are not. Nothing in this repository contains card names
//! or text — the database is populated at first run from Scryfall's bulk export. See
//! `mtg-oracle`.

pub mod cards;
pub mod decks;
pub mod identity;
pub mod schema;

use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// Re-exported so callers that need a transaction-scoped write path (the bulk importer)
/// can name the types without taking their own dependency on a specific rusqlite version.
pub use rusqlite;

pub use cards::{CardQuery, FaceRow, StoredCard};
pub use decks::{DeckRow, Section};
pub use identity::{StoredFriend, StoredIdentity};

/// Where the database lives: `$MTGO_RS_DB`, else `$XDG_DATA_HOME/mtgo_rs/cards.sqlite`, else
/// `~/.local/share/mtgo_rs/cards.sqlite`.
///
/// Here rather than in a frontend so the CLI that imports and the window that plays agree on
/// one file without either knowing about the other.
pub fn default_path() -> PathBuf {
    if let Ok(explicit) = std::env::var("MTGO_RS_DB") {
        return PathBuf::from(explicit);
    }
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local/share")
        });
    base.join("mtgo_rs").join("cards.sqlite")
}

/// A handle on the local database.
pub struct Store {
    conn: Connection,
}

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    /// The file was written by a newer version of the client.
    SchemaTooNew {
        found: u32,
        supported: u32,
    },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Sql(e) => write!(f, "database error: {e}"),
            StoreError::SchemaTooNew { found, supported } => write!(
                f,
                "database schema v{found} is newer than this build supports (v{supported})"
            ),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sql(e)
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

impl Store {
    /// Open (creating if absent) and bring the schema up to date.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::prepare(conn)
    }

    /// An in-memory database, for tests.
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::prepare(conn)
    }

    fn prepare(conn: Connection) -> Result<Self> {
        // WAL keeps a long import from blocking reads, and foreign keys are off by
        // default in SQLite — both are per-connection, so they are set on every open
        // rather than once at creation.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        let mut store = Self { conn };
        schema::migrate(&mut store)?;
        store.intern_creature_types()?;
        Ok(store)
    }

    /// Intern every creature type, not only those printed on a card: some appear only on
    /// tokens ("create a 1/1 colorless Eldrazi Scion creature token"), and rules text can
    /// name a type only once it has an id. Idempotent, so it runs on every open and an
    /// existing database needs no re-import.
    fn intern_creature_types(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut insert =
                tx.prepare("INSERT INTO subtypes (name) VALUES (?1) ON CONFLICT(name) DO NOTHING")?;
            for name in mtg_core::creature_types::CREATURE_TYPES {
                insert.execute([name])?;
            }
            // Artifact types printed only on tokens (CR 205.3g).
            for name in ["Blood", "Gold", "Incubator", "Junk", "Map", "Powerstone"] {
                insert.execute([name])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Read a value from the `meta` table.
    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        let mut q = self.conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = q.query([key])?;
        Ok(match rows.next()? {
            Some(r) => Some(r.get(0)?),
            None => None,
        })
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            (key, value),
        )?;
        Ok(())
    }
}
