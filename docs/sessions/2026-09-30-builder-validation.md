---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, client]
---

# Deck validation uses the active builder database

Continued autonomously alongside Claude. Code edits are confined to
crates/mtg-app/src/builder.rs, leaving concurrent rules changes untouched.

Builder::check previously constructed a LocalSource by reopening the default
database even though it received an active Store. An in-memory or alternate store
could therefore show cards in the builder but fail validation, or validate against
unrelated data. A borrowed BuilderSource now reads card rows, subtype ids, face
layouts, compiled faces, and colour identities from the passed Store. GameCards
still remaps source subtype ids normally, and existing deck_problems remains the
source of structural checks.

Before building a DeckSpec, validation checks unique ids from main deck, sideboard,
and commander for missing rows and read failures. These receive explicit messages
with cached names and card ids. Draft::spec's filter_map cannot silently drop missing
cards from validation. Subtype-table failures also produce an explicit message.

The deck panel still shows its first three issues immediately, and now offers a
collapsible list of every remaining problem instead of an unexpandable count.
No match rules, format constraints, compiler gates, or persistence behavior changed.

Validation:
- cargo test -p mtg-app --lib builder::tests: 28 passed.
- cargo test -p mtg-app --lib: all 105 passed.
- cargo clippy -p mtg-app --all-targets -- -D warnings: clean.
- Four new regressions verify an in-memory valid Constructed deck, actual size/
  sideboard/copy issues, Commander colour identity from local rows, and missing
  entries across every section before spec conversion can omit them.
- git diff --check -- crates/mtg-app/src/builder.rs: clean.
- GUI rebuilt with cargo build -p mtg-app --bin mtg-gui. Interactive GUI not run.

Claude observed a temporary adapter signature mismatch during implementation;
Subtypes::intern now correctly returns Option<u16>, and all checks above pass.
No workspace-wide test or coverage claim made during concurrent rules work.
No user collection modified and no commit created by Codex. Brain remains outside
writable roots; handoff and coordination remain in the repository.
