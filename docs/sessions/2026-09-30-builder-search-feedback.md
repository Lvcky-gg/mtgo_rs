# Deck-builder search feedback

Updated `crates/mtg-app/src/builder.rs` to distinguish a database with no
legality metadata from a query that simply has no matches. The metadata check
now looks across the database, independent of the active text/type/color search.
The empty-page message also identifies the active legality filter.

Pagination now fetches one extra result to determine whether a next page exists,
so an exact multiple of the page size no longer offers a blank final page.

Validation: `cargo check -p mtg-app`, `cargo clippy -p mtg-app --all-targets --
-D warnings`, `cargo build -p mtg-app --bin mtg-gui`, and `git diff --check`
passed. No tests were run.
