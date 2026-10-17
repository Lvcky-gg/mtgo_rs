# Deck card coverage — 2026-10-16

Rechecked the saved 209-card priority list against a SQLite backup of the local
catalog. The starting code supported 205 / 209; the saved report was stale.
Phelia, Exuberant Shepherd now compiles, raising priority coverage to 206 / 209.
The refreshed report is `docs/card-coverage/deck-priorities-report.txt`.
Remaining blockers: Cursed Mirror, Preston, the Vanisher, and Slimed.

Phelia's return and conditional counter instruction are compiled together inside
the delayed end-step trigger. The condition checks the returned object's new
identity, battlefield destination, and controller. Returning an opponent-owned
permanent does not grant the counter, even if you controlled it before exile.
No new engine state or replay fixture changes are needed.

Four compiled gameplay regressions cover return/counter timing and ownership,
attacking without a legal target, removing the card from exile before the delayed
return, and blinking Phelia before the return (the old ability returns the card
but does not put a counter on Phelia's new incarnation).

Preston remains conservatively blocked: its copy exceptions must be copiable
values that apply before entry and carry through subsequent copies, rather than
ordinary continuous effects after creating the token.

Changes remain uncommitted.

Whole-catalog compilation coverage on this snapshot: 18,909 / 34,954 (54.1%).
This is a net count on the current code and catalog, not a per-card regression
comparison with earlier sessions. Compilation coverage is not a full rules audit.

Validation: `cargo test --workspace --quiet`, the four focused Phelia tests,
`cargo clippy -p mtg-oracle --lib -- -D warnings`, `cargo fmt --all -- --check`,
and `git diff --check` pass. The full workspace/all-target Clippy check is blocked
by pre-existing warnings: nested tapped-entry conditionals in engine resolve,
example code in selfplay/deck_pool/rewind_sweep/tune_eval/arena, and a boolean
assertion in `empty_library_wins.rs`. Those unrelated files remain unchanged.
