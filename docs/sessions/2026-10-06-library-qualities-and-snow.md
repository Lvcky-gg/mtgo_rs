# Library card qualities and snow filters

A shared library-card filter parser now handles bare adjectives such as
`legendary`, `nonlegendary`, `nonland` and colors for searches and dig selection.
Bare qualities can include instants and sorceries. When `permanent` is printed,
the parser explicitly intersects the quality with permanent card types.

This also fixes previously accepted United Battlefront-style selection offering
instants and sorceries: `noncreature, nonland permanent` now excludes them.
The noun adjective parser recognizes `snow` and `nonsnow` using the existing
`HasSupertype(Snow)` filter and its complement, enabling searches, targets,
counts and conditional effects without adding an engine primitive.

## Target-source correction

The Rime Tender regression exposed an existing `another target` bug for abilities:
targeting evaluated `IsSelf` against the object representing the ability on the
stack. Targeting contexts now use the actual ability source for filters and
resolution rechecks. The explicit self-target exclusion applies to stack objects;
a permanent can be targeted by its own ability unless the printed text excludes
it. A second regression checks both self-targeting and `another` when the source
is the only creature on the battlefield.

Claude's round-14 divided-damage/casting work was left untouched. Edits in the
shared targeting file are confined to context construction and self exclusion.

## Measured coverage

A frozen source snapshot at `/tmp/mtgo-card-quality-comparison` was measured
against the existing isolated 34,913-card database. Only the compiler changes
for this batch differed between the two builds; Claude's work stayed constant:
**15,523 → 15,560 playable cards (44.6%), +37, zero lost card faces**.

Artifacts: `/tmp/codex-qualities-frozen-before.txt` and
`/tmp/codex-qualities-frozen-after.txt`. This measures complete printed-text
compilation, not an exhaustive gameplay audit of every accepted interaction.
The user's original database was not changed; no reimport is required.

## Validation

Ten new gameplay regressions in `compiled/library_qualities.rs` check actual
candidate lists and results for Captain Sisay, Unmarked Grave, color/nonland
searches, United Battlefront, Into the North, Glacial Revelation, Dead of Winter,
explicit permanent restrictions, Rime Tender and source-self-targeting abilities.
Tests include optional failed searches, basic and nonbasic snow lands, tapped
entry, mana-value boundaries, legendary instants, hypothetical snow instants
and sorceries, public reveal counts, ownership in snow counts, and source exclusion.
The existing dig selection helper was shared with this module.

Passed:

- `cargo test -p mtg-oracle -p mtg-engine -p mtg-ir -p mtg-policy --offline --quiet`
  (1,092 passed, 38 existing tests ignored).
- `cargo clippy -p mtg-oracle -p mtg-engine --all-targets --offline -- -D warnings`
- `cargo build -p mtg-app --bin mtg-gui --offline --quiet`
- `git diff --check`

## Newly supported cards

- Abominable Treefolk
- Arctic Foxes
- Avalanche Caller
- Balduvian Conjurer
- Blizzard
- Captain Sisay
- Chill to the Bone
- Conifer Wurm
- Dead of Winter
- Drift of the Dead
- Freyalise's Radiance
- Frost Bite
- Gangrenous Zombies
- Glacial Crevasses
- Glacial Revelation
- Guardian Sunmare
- Hallowed Ground
- Heidar, Rimewind Master
- Iceberg Cancrix
- Into the North
- Karplusan Giant
- On Thin Ice
- Priest of the Haunted Edge
- Rime Tender
- Rimewind Cryomancer
- Rimewind Taskmage
- Ronom Serpent
- Sculptor of Winter
- Selective Memory
- Skred
- Spirit of the Aldergard
- Sunstone
- Thalia's Lancers
- Unmarked Grave
- Whiteout
- Winter's Rest
- Woolly Mammoths
