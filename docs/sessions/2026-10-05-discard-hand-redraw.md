# Discard hand and redraw — 2026-10-05

The compiler now supports “discard all the cards in your hand, then draw that many cards,” the “discard your hand” synonym, and fixed “plus one”/other fixed-number followups. An existing Effect::Let snapshots the hand into a dedicated binding before discarding. The draw counts that binding's original objects after the cards have moved, so it retains the original quantity. No engine implementation changes were needed.

This enables text used by Tolarian Winds, Shattered Perception, and Fateful Showdown. Following effects continue to compose normally. Optional or conditional variants outside this grammar remain unsupported.

Three regressions cover complete parsing/rejection; empty and three-card hands with and without the extra draw, preserving an opponent's hand; and damage based on the original hand before the discard/redraw sequence. Casting the spell itself does not inflate the hand count.

Coverage: 13,992 → 13,998 / 34,913 cards (40.1%). Universal support remains incomplete.

Validation: all-feature oracle tests passed (81 unit, 459 compiled-game, 35 import, 10 import-atomicity; 585 enabled total, 15 ignored). All-target/all-feature oracle Clippy with warnings denied and whitespace checks passed. Log: `/tmp/mtgo-hand-redraw-tests.log`. Concurrent edits preserved; changes remain uncommitted.
