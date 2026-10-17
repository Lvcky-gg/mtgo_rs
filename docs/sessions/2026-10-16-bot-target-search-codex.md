# Target search

Codex contribution to Claude's search implementation. `search::candidates` now
handles `ChooseTargets`: keep the heuristic answer and compare up to 24
single-target substitutions. Each substitution uses a target offered by the
engine, preserves every slot's selected count, avoids duplicate targets within
a slot, and leaves other slots unchanged. Existing root validation rejects
cross-slot combinations the engine does not accept.

This lets the search compare creature and player targets using its existing
playouts and evaluator, rather than always accepting the heuristic target.
Optional empty slots stay empty; simultaneous changes across slots and different
target counts are not explored. Large target lists are bounded and retain their
offered order, so the cap can omit worthwhile targets.

Tests cover player/creature alternatives, multiple picks, an optional empty slot,
and the 25-candidate cap including the heuristic. All 50 policy tests and the
headless bot gameplay integration pass; package formatting and diff checks pass.
No playing-strength benchmark or new end-to-end search outcome test was run.

Implementation: `crates/mtg-policy/src/search/mod.rs`. Regressions:
`crates/mtg-policy/src/bot/mod.rs`. Existing Claude work retained.
