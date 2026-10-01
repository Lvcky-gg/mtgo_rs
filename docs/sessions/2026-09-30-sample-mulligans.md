---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, client]
---

# London mulligan practice in sample hands

Continued autonomously alongside Claude. Code changes are confined to
crates/mtg-app/src/builder.rs; Claude's current rules work was left untouched.

The sample-hand window now has a dedicated SampleHand model rather than a prefix
of a shuffled vector. Users may mulligan into fresh seven-card hands, select the
required individual copies to bottom, keep the reduced hand, and draw afterward.
Duplicate cards are selected by hand position. Selection order determines bottom
order, with numbered markers and a dark backing for readability. Drawing is
disabled until keeping, and mulligans stop once the resulting hand is empty.
New hand resets the practice sequence.

An optional first-free-mulligan checkbox supports multiplayer/Brawl practice; it
defaults off for a normal duel and can be changed only before the first mulligan.
It is independent of match rules. The library retains unselected cards followed
by bottomed cards, so later draws reach bottomed cards only after the rest.
Library counts and land counts are visible, and the window scrolls for later draws.

Composition changes invalidate an open sample and prompt generation of a fresh
one. Renaming leaves it intact. Sample data never changes the saved deck, and
the existing main-only shuffle continues to exclude sideboard and commander.

Validation:
- cargo test -p mtg-app --lib builder::tests: 24 passed.
- cargo test -p mtg-app --lib: all 101 passed.
- cargo clippy -p mtg-app --all-targets -- -D warnings: clean.
- Five new tests cover draw/keep sequencing, bottom order and conservation,
  duplicate-copy selection, redraw reset, zero-card stopping/free first mulligan,
  initial keep/library exhaustion, and composition invalidation versus rename.
- GUI rebuilt with cargo build -p mtg-app --bin mtg-gui. Interactive GUI not run.

Verified CR 103.5 and 103.5c in the June 19 2026 Comprehensive Rules:
https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf

No workspace-wide test or coverage claim made during concurrent rules work.
No user collection modified and no commit created by Codex. Brain is outside
writable roots; coordination and handoff remain in the repository.
