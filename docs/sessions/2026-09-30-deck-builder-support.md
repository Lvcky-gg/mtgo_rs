---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, client]
---

# Card support explained in the deck builder

Continued autonomously alongside Claude, who owns ongoing changeling, prevention,
granted-ability and copy work. Code changes remain confined to
crates/mtg-app/src/builder.rs. Shared rules files and README were left untouched.

Search tiles now display “Rules incomplete” for cards that fail the same whole-card
support gate used by match coverage. Hovering a card explains unsupported printed
text from every face, unsupported face layouts, or unsupported split ability
shapes. Fully supported cards get a positive hover label. Database read errors and
missing cards produce explicit reasons instead of false support claims.

The deck panel summarizes supported unique cards across main deck, sideboard and
commander. A collapsible list names all affected cards and their reasons. Deck rows
show a warning indicator, and commander/card hover previews include support status.
Adding, saving, undoing and playtesting remain available; this exposes limitations
without preventing users from building the decks they want.

Per-card status is cached for the builder's lifetime. The cache is independent of
deck history and applies to cards restored by undo. No card data or compiler
behavior was changed. An unexplained gate failure receives a generic warning so
future gate additions cannot silently appear supported.

Validation:
- cargo test -p mtg-app --lib builder::tests: 19 passed.
- cargo test -p mtg-app --lib: all 96 passed.
- cargo clippy -p mtg-app --all-targets -- -D warnings: clean.
- Three new regressions check whole-card gate agreement/missing cards, unsupported
  text on the other MDFC face, and unsupported layouts with supported rules text.
- git diff --check -- crates/mtg-app/src/builder.rs: clean.
- GUI rebuilt with cargo build -p mtg-app --bin mtg-gui. Interactive GUI not run.

No workspace-wide test or card-coverage claim made during concurrent rules work.
No user collection modified and no commit created by Codex. Coordination is in
docs/coordination.md; Brain remains outside writable roots.
