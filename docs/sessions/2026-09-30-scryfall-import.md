---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, persistence]
---

# Scryfall database import appeared stuck

User reported Download from Scryfall stuck on import. Reproduced a live import in
an isolated /tmp database: it did eventually complete after roughly eight minutes.
The actual Scryfall manifest provides oracle_cards via jsonl_download_uri, a gzip
JSON Lines file approximately 24.6 MB compressed. Network and parsing worked.

Found two quadratic update paths, confirmed by EXPLAIN QUERY PLAN on a read-only
copy of the existing schema: DELETE FROM face_keys WHERE oracle scanned face_keys,
and DELETE FROM card_search WHERE oracle scanned the FTS virtual table's unindexed
oracle column. Both ran for every imported card as the tables grew.

Schema v6 adds face_keys_oracle and rebuilds FTS documents with rowid = oracle.
Both Store::put_card and the transaction-scoped importer now delete by indexed FTS
rowid and insert with that stable id. Existing cards, ids, face data and decks are
preserved; only search documents are rebuilt. Old clients refuse schema v6 rather
than writing an incompatible database.

Added streaming importer progress every 500 rows and before commit. Counts include
skipped/malformed input rows; gzip, JSON Lines and JSON arrays share the callback.
Fetch progress includes manifest lookup, connection, rows/cards, response bytes
and total size when available, and saving. GUI and CLI report progress. GUI handles
worker disconnection instead of leaving the spinner active indefinitely. File
imports now use the same timestamp wrapper as downloads, so daily refresh policy
also applies to GUI file import. HTTP resolution, connection, headers and complete
body waits are bounded (10s, 15s, 30s, 10min respectively).

Verified new live download/import to /tmp/mtgo-scryfall-fixed.sqlite: 12.745 seconds
wall time, 34,898 imported, 3,792 skipped, zero failed. Search documents match card
count and all rowids match oracle ids; SQLite quick_check is ok. Migrated the prior
read-only backup of the user's database in /tmp: all 34,898 cards retained, matching
search documents and quick_check ok. The real collection was not written.

Validation: cargo test --workspace --all-features: 697 passed. cargo clippy
--workspace --all-targets --all-features -- -D warnings: clean. New tests cover
migration from old mismatched/duplicate search rowids while preserving a deck,
indexed name-key updates, duplicate-free reimport, progress for arrays/lines/gzip,
periodic row updates, and rollback on truncated input. GUI rebuilt with cargo build
-p mtg-app --bin mtg-gui; user must restart a running old binary to use the changes.
No interactive GUI test performed.

Prior task completed this turn: MDFC casting, 693 tests, 8,499 / 34,898 fully playable
cards (24.4%); see 2026-09-30-mdfc-casting.md. Remaining gameplay work: split/adventure/
transform cards, damage prevention/copies, and shuffle-seed information leak.

Brain remains outside writable roots, so handoffs are in docs/sessions. No commit
created; repository sources remain untracked.
