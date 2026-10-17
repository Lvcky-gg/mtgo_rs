# Modal spell search

Codex contribution alongside Claude's search work. `search::candidates` now
handles `ChooseModes`, comparing combinations between the offered minimum and
maximum counts. The helper in `search/modes.rs` emits at most 24 alternatives
without repeated indices, plus the existing heuristic candidate. Existing root
validation checks engine legality before playouts.

Four unit tests cover exact pairs, choose-one-or-both, optional zero modes,
impossible counts, and a large combination space. An actual compiled-card test
in `choose_both.rs` offers default gain-three-life versus target-opponent-loses-
three-life with the opponent at three. Search selects mode 1 and the engine
confirms the opponent loses while the bot survives.

Validation: 54 policy tests pass; the compiled-card search outcome regression
passes; headless bot gameplay integration passes; formatting and diff checks pass.

Combinations are enumerated by increasing count, then lexicographic order. Large
modal sets can omit good combinations after the cap. Playouts still use the
heuristic for subsequent mode choices. No playing-strength benchmark was run.
