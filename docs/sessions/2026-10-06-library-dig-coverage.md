# Library selection card support

Codex extended only `clauses::dig`, leaving Claude's concurrent casting/cost
mechanics untouched. The shared claims and validation notes are in
`docs/coordination.md`.

The compiler now recognizes `Put up to two … from among them onto the battlefield`
without requiring a leading `you may`, while preserving the optional count.
Required reveal/take instructions remain required when a matching card exists.
It also accepts `cards of the chosen type` and shortened `on the bottom in a
random order` / `in any order` destinations. These use existing engine operations.

## Measured coverage

On the same isolated 34,913-card database snapshot, with Claude's concurrent
changes already present in both measurements:
**15,400 → 15,404 playable cards (44.1%), +4, zero lost card faces**.

Newly supported cards:

- Collected Company
- Deploy the Gatewatch
- Icon of Ancestry
- United Battlefront

This measures complete text compilation, rather than an exhaustive gameplay
audit. The user's database was not modified; no reimport is needed.

## Validation

Four new gameplay regression tests (six total in `compiled/dig.rs`) cover
chosen-type selection, abbreviated bottom destinations, required selection with
and without matches, and Collected Company's zero/one/two selections. The latter
also checks the offered candidates, the mana-value boundary, battlefield entry,
and bottom ordering.

Passed:

- `cargo test -p mtg-oracle --test compiled dig:: --offline --quiet`
- `cargo test -p mtg-oracle -p mtg-engine -p mtg-ir -p mtg-policy --offline --quiet`
- `cargo clippy -p mtg-oracle --all-targets --offline -- -D warnings`
- `cargo build -p mtg-app --bin mtg-gui --offline --quiet`

A temporary compile error in concurrent casting work was recorded in the shared
handoff and resolved before the final checks.
