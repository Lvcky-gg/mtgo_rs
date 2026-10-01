//! Storing this installation's identity and its pinned peers.
//!
//! The store deals in **bytes**, not keys. It has no crypto dependency and no opinion about
//! what a 32-byte seed means — `mtg-net` owns that. Keeping the boundary there means the
//! database schema does not change if the signature scheme ever does.
//!
//! The private seed is stored unencrypted, which is a deliberate and stateable choice: the
//! threat model is a lost or shared *link*, not a compromised machine, and a passphrase on
//! every launch would be security theatre for a game client. Anyone with read access to the
//! file can impersonate this identity — the same as for an SSH key.

use rusqlite::{OptionalExtension, params};

use crate::{Result, Store};

/// A stored keypair, as raw bytes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredIdentity {
    pub seed: [u8; 32],
    pub public: [u8; 32],
}

/// A trusted peer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredFriend {
    pub public: [u8; 32],
    pub nickname: String,
}

impl Store {
    /// This installation's identity, if one has been generated.
    pub fn identity(&self) -> Result<Option<StoredIdentity>> {
        let row = self
            .conn()
            .query_row("SELECT seed, public FROM identity WHERE id = 1", [], |r| {
                Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .optional()?;

        Ok(row.and_then(|(seed, public)| {
            Some(StoredIdentity {
                seed: seed.try_into().ok()?,
                public: public.try_into().ok()?,
            })
        }))
    }

    /// Store the identity, replacing any existing one.
    ///
    /// Replacing is deliberate rather than an error: restoring from a backup should just
    /// work. The caller is responsible for warning first, because doing it by accident
    /// discards an identity that cannot be recovered.
    pub fn put_identity(&self, id: &StoredIdentity) -> Result<()> {
        self.conn().execute(
            "INSERT INTO identity (id, seed, public, created) VALUES (1, ?1, ?2, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET seed = excluded.seed, public = excluded.public",
            params![id.seed.as_slice(), id.public.as_slice()],
        )?;
        Ok(())
    }

    /// Pin a peer, or rename one already pinned.
    pub fn put_friend(&self, public: &[u8; 32], nickname: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO friends (public, nickname, first_seen)
             VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(public) DO UPDATE SET nickname = excluded.nickname",
            params![public.as_slice(), nickname],
        )?;
        Ok(())
    }

    pub fn friend(&self, public: &[u8; 32]) -> Result<Option<StoredFriend>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT public, nickname FROM friends WHERE public = ?1",
                params![public.as_slice()],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
            .and_then(|(p, nickname)| {
                Some(StoredFriend {
                    public: p.try_into().ok()?,
                    nickname,
                })
            }))
    }

    pub fn friends(&self) -> Result<Vec<StoredFriend>> {
        let mut q = self
            .conn()
            .prepare("SELECT public, nickname FROM friends ORDER BY nickname")?;
        let rows = q.query_map([], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (p, nickname) = r?;
            if let Ok(public) = p.try_into() {
                out.push(StoredFriend { public, nickname });
            }
        }
        Ok(out)
    }

    pub fn forget_friend(&self, public: &[u8; 32]) -> Result<()> {
        self.conn().execute(
            "DELETE FROM friends WHERE public = ?1",
            params![public.as_slice()],
        )?;
        Ok(())
    }
}
