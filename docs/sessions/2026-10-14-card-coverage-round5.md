# Card coverage, round 5 — 2026-10-14

Using the same `/tmp/card-coverage.sqlite` snapshot, coverage rises from 17,654
to 17,662 / 34,914 (50.6%). The complete card-name comparison gains eight cards
and loses none. The existing five-card name-lookup difference remains
(17,667 versus 17,662).

## Spell-sequence triggers

`EventPattern::NthSpellCast` now supports an opponent-turn restriction and an
at-least ordinal. The compiler recognizes `your first spell during each
opponent's turn` and `a spell other than your first spell each turn`.
The latter triggers on the second and every subsequent spell, rather than only
the second. Existing exact ordinal and noncreature-count behavior stays intact.
New flags default to false and are omitted from serialized legacy patterns.

The turn restriction is checked inside the event-pattern matcher, keeping its
qualification local even when a pattern occurs within a combined trigger.
Counts use actual casts by the selected player this turn. Spell copies do not
count as casts, and countering a spell does not undo its cast count.

Newly playable: Arcbound Tracker; Arena Trickster; Dreamstalker Manticore;
Gustha Ebbasdotter; Mischievous Chimera; Nymris, Oona's Trickster;
Stinging Lionfish; Wavebreak Hippocamp.

Six compiled gameplay regressions cover first-cast triggers on two distinct
opponents' turns, exclusion of the controller's own turn and other players'
casts, a source entering after the first spell, a countered first spell,
spell copying, and repeated after-first triggers with a fresh count next turn.

Validation: targeted compiled tests; `cargo test --workspace --quiet` including
the replay/regression corpus; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all --check`; and `git diff --check`. Workspace tests run outside
the sandbox for their existing loopback networking tests. Changes remain uncommitted.
