# Bot blocking priorities

Codex contribution alongside Claude's search work. Changes are in
`crates/mtg-policy/src/bot/block_strategy.rs`, with two regressions in
`bot/mod.rs`.

Safe blocks now consider attackers aimed at the player before those aimed at
planeswalkers. Previously a larger walker attacker could consume the only
defender, even when that defender could safely kill the smaller lethal attacker
aimed at the player. Among safe blockers, choose the blocker with fewer eligible
attackers first; this preserves a flexible flyer for a flying attacker instead
of wasting it on a ground attacker that another defender can safely kill.

Regressions exercise both cases with lethal incoming player damage. Verification:
`cargo test -p mtg-policy` (47 passed including current search tests),
`cargo test -p mtg-headless --test bot` (1 passed), formatting and diff checks.

This is a greedy improvement, not a full block assignment optimizer. In particular,
safe blocks can still compete with chumps and menace groups for scarce defenders.
Existing unrelated working changes retained.
