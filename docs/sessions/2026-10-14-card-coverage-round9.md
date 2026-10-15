# Card coverage, round 9 — 2026-10-14

Using the same `/tmp/card-coverage.sqlite` snapshot, coverage rises from 17,701
to 17,713 / 34,914 (50.7%). The complete card-name comparison gains 12 cards
with zero losses. The existing five-card name-lookup difference remains
(17,718 versus 17,713).

## Cycling lists

Comma-separated cycling lines compile into independent abilities. Each entry
uses a fresh parsing context and the entire list must parse. Activating one
ability pays its own cost and discards the card once; it searches for its own
printed quality rather than combining the choices into one search.

Newly playable: Igneous Pouncer; Jhessian Zombies; Pale Recluse;
Sanctum Plowbeast; Valley Rannet.

## Combined events and commander nouns

Generic filtered subjects accept `enters or attacks`, producing an `AnyOf`
event pattern with the same filter on both branches. This supports Kindred
Discovery's chosen-type and controller filter without separate trigger text.

The noun parser recognizes singular and plural commanders through the existing
commander identity filter. Controller qualifications remain independent of
the card's commander designation. This also supports commander-only anthems
and targeting.

Newly playable: Anara, Wolvid Familiar; Falthis, Shadowcat Familiar;
Keleth, Sunmane Familiar; Kindred Discovery; Norn's Choirmaster;
The Dilu Horse; Witch's Clinic.

Six compiled gameplay regressions cover distinct Plains/Island cycling abilities,
nonbasic lands with the requested type, discard/cycle/reveal/shuffle behavior,
failed hidden-zone searches, unavailable cycling outside the hand, Kindred
Discovery's chosen type on entry and attack, commander anthems and own-turn
conditions, and Norn's Choirmaster's proliferate on commander entry and attack.

Validation: targeted compiled tests; `cargo test --workspace --quiet` including
the replay/regression corpus; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all --check`; and `git diff --check`. Workspace tests run outside
the sandbox for their existing loopback networking tests. Changes remain uncommitted.
