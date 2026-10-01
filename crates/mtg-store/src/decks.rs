//! Deck storage.
//!
//! A deck is a name plus counted oracle ids per section. Deliberately not a snapshot of
//! card data: if a card is later re-imported with corrected text, every deck referring to
//! it should see the correction, which only works if decks store identities rather than
//! copies.

use rusqlite::params;

use crate::{Result, Store};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Main,
    Sideboard,
    Commander,
}

impl Section {
    fn as_str(self) -> &'static str {
        match self {
            Section::Main => "main",
            Section::Sideboard => "sideboard",
            Section::Commander => "commander",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "sideboard" => Section::Sideboard,
            "commander" => Section::Commander,
            _ => Section::Main,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeckRow {
    pub id: i64,
    pub name: String,
    pub format: Option<String>,
    /// (oracle id, count, section)
    pub entries: Vec<(u32, u32, Section)>,
}

impl Store {
    /// Save a deck, replacing any deck with the same name.
    pub fn put_deck(
        &mut self,
        name: &str,
        format: Option<&str>,
        entries: &[(u32, u32, Section)],
    ) -> Result<i64> {
        self.put_deck_replacing(None, name, format, entries)
    }

    /// Save a deck and retire its previous saved ID atomically, including renames.
    /// Existing same-name replacement behavior is retained.
    pub fn put_deck_replacing(
        &mut self,
        previous_id: Option<i64>,
        name: &str,
        format: Option<&str>,
        entries: &[(u32, u32, Section)],
    ) -> Result<i64> {
        let tx = self.conn.transaction()?;

        // Replacing by name is what re-pasting an edited list should mean. Entries go
        // with it via ON DELETE CASCADE.
        tx.execute("DELETE FROM decks WHERE name = ?1", params![name])?;
        if let Some(id) = previous_id {
            tx.execute("DELETE FROM decks WHERE id = ?1", params![id])?;
        }
        tx.execute(
            "INSERT INTO decks (name, format, created) VALUES (?1, ?2, datetime('now'))",
            params![name, format],
        )?;
        let id = tx.last_insert_rowid();

        for (oracle, count, section) in entries {
            tx.execute(
                "INSERT INTO deck_entries (deck, oracle, count, section) VALUES (?1, ?2, ?3, ?4)",
                params![id, oracle, count, section.as_str()],
            )?;
        }

        tx.commit()?;
        Ok(id)
    }

    pub fn deck(&self, id: i64) -> Result<Option<DeckRow>> {
        use rusqlite::OptionalExtension;
        let head = self
            .conn
            .query_row(
                "SELECT id, name, format FROM decks WHERE id = ?1",
                params![id],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, name, format)) = head else {
            return Ok(None);
        };

        let mut q = self
            .conn
            .prepare("SELECT oracle, count, section FROM deck_entries WHERE deck = ?1")?;
        let rows = q.query_map(params![id], |r| {
            Ok((
                r.get::<_, i64>(0)? as u32,
                r.get::<_, i64>(1)? as u32,
                Section::parse(&r.get::<_, String>(2)?),
            ))
        })?;
        let mut entries = Vec::new();
        for r in rows {
            entries.push(r?);
        }

        Ok(Some(DeckRow {
            id,
            name,
            format,
            entries,
        }))
    }

    /// Every saved deck's id and name, newest first.
    pub fn deck_list(&self) -> Result<Vec<(i64, String)>> {
        let mut q = self
            .conn
            .prepare("SELECT id, name FROM decks ORDER BY created DESC, id DESC")?;
        let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_deck(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM decks WHERE id = ?1", params![id])?;
        Ok(())
    }
}
