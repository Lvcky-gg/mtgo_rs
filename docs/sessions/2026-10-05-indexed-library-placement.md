# Indexed library placement — 2026-10-05

The compiler now accepts putting a permanent or supported graveyard target into its owner's library second or third from the top. It also recognizes graveyard activated abilities that put the card itself into its owner's library at those positions, enabling Bookwurm and Screaming Swarm shapes.

Existing ZonePosition::FromTop and engine insertion implement the behavior. Text ordinals map to zero-based positions (second → 1, third → 2). No engine implementation changes were needed. Owner-choice alternatives and other unsupported variants continue to be rejected.

Three regression tests cover accepted/rejected grammar, exact owner-library placement without disturbing other cards, and graveyard activation with full, empty, and one-card libraries. Short libraries place the card at the end.

Coverage: 13,998 → 14,004 / 34,913 cards (40.1%). Universal support remains incomplete.

Validation: all-feature oracle suite passed (82 unit, 461 compiled-game, 35 import, 10 import-atomicity; 588 enabled total, 15 ignored). All-target/all-feature oracle Clippy with warnings denied and whitespace checks passed. Log: `/tmp/mtgo-library-positions-tests.log`. Concurrent edits preserved; changes remain uncommitted.
