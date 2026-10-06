# Import transaction and streaming follow-up

Continued the reliability audit in `mtg-oracle/src/import.rs`, `fetch.rs`, and
`tests/import_atomicity.rs`. Existing edits in other areas were preserved.

File and download import wrappers now commit `last_import_unix` in the same
transaction as cards and `cards_imported`. A timestamp-write failure previously
returned an error after changing the database; the regression reproduced that
behavior and now verifies preservation of both cards and metadata. The optional
timestamp is carried through compressed imports too. Bare `mtg_oracle::import`
continues to omit the refresh timestamp.

`fetch::last_import` now returns `Result<Option<u64>, FetchError>`, distinguishing
database read failures from missing or unparsable advisory timestamps. The only
workspace caller propagates the error before attempting a download. Invalid text
still behaves as absent metadata, preserving the previous refresh policy.

Gzip detection reads and replays a two-byte prefix rather than assuming the first
`Read` call fills both header bytes. A one-byte-at-a-time reader reproduced the
old failure and now imports correctly. Nested gzip decoding is bounded to four
layers; the boundary succeeds and a fifth layer returns an error without writes.

The importer uses `MultiGzDecoder`. The previous single-member decoder silently
imported only one of two concatenated gzip members. A JSON Lines regression now
verifies both cards arrive. A truncated later member aborts and rolls back earlier
rows, including import metadata, instead of appearing to complete successfully.

Targeted validation: 76 oracle library tests, 35 existing import tests, and all
10 atomicity/streaming regressions pass. The follow-up adds seven tests in total:
six integration regressions and one refresh-metadata test. Final
`cargo test --workspace --all-features --quiet`: 1,250 passed, zero failed,
41 ignored across 62 test/doc-test groups. Final workspace Clippy with all
targets/features and warnings denied passes, as does `git diff --check`.
No live bulk download, manual GUI test, or commit was performed.
