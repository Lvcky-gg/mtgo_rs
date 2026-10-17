# Optional effect search

Codex contribution alongside Claude's search implementation. `search::candidates`
now handles `ChoiceKind::Confirm`, offering both `Answer::Bool(false)` and
`Answer::Bool(true)` alongside the heuristic. Existing deduplication and engine
validation leave at most two roots; the search compares their playout outcomes.

An actual compiled sorcery ("You may draw a card.") is tested in
`bot_blasts.rs` with a populated and an empty library. Search accepts the useful
draw, declines the fatal draw, and actual resolution confirms the expected hand
size and that the bot survives in both cases.

Validation: 54 policy tests and the compiled-card regression (both library cases)
pass; formatting and diff checks pass. The ordinary heuristic bot and subsequent
heuristic playout choices retain their existing confirmation policy. Decisions
remain limited by the search's horizon and evaluator. No strength benchmark run.
