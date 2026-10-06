# Mana abilities with additional effects — 2026-10-05

The compiler now recognizes mana abilities followed by fixed, unconditional draws, life gain, or life loss. It reuses the existing mana-ability compiler for the initial sentence and appends the additional effects while preserving mana-ability classification, costs, timing, and color alternatives. The engine already resolves these effects immediately, including when an automatic payment activates the source.

This covers shapes such as Chromatic Sphere's paid sacrifice ability and mana-producing sacrifices that also draw a card. Targeted, optional, discard-choice, and variable-count followups remain rejected by this path. The compiler does not claim those effects are playable without the necessary choice handling.

Three new tests cover compiler acceptance/rejection, paid manual activation with chosen color and immediate draw, and an automatic spell payment that sacrifices its mana source and draws. No engine implementation changes were made. Concurrent changes were preserved.

Local database coverage: 13,971 → 13,978 of 34,913 cards (40.0%). Universal support remains unfinished.

Validation: all oracle tests with all features passed (78 unit, 450 compiled-game, 35 import, 10 import-atomicity; 15 ignored). All-target/all-feature oracle Clippy with warnings denied and whitespace checks passed. Log: `/tmp/mtgo-mana-followup-tests.log`. Changes remain uncommitted.
