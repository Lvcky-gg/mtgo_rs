# Card coverage, round 8 — 2026-10-14

Using the same `/tmp/card-coverage.sqlite` snapshot, coverage rises from 17,690
to 17,701 / 34,914 (50.7%). The complete card-name comparison gains 11 cards
with zero losses. The existing five-card name-lookup difference remains
(17,706 versus 17,701).

## Additive characteristics

The modification parser accepts a colored subtype with `in addition to its
other colors and types`, emitting the existing additive color and subtype
modifications. It also accepts `other creature types` as the printed spelling
for retained creature subtypes. Existing colors, card types, and creature types
are preserved.

A static `is a creature with base power and toughness … in addition to its other
types` adds the creature card type and sets base power/toughness. The subject's
other card types and abilities remain intact. The fixed-color one-shot shape
`becomes blue in addition to its other colors until end of turn` uses the existing
temporary additive color modification.

Newly playable: Angelic Armaments; Dread Slaver; Ensoul Artifact; Ghoulflesh;
Graaz, Unstoppable Juggernaut; Hivestone; Indigo Faerie; Liliana, Death's Majesty;
Mephidross Vampire; Mightstone's Animation; Rise from the Grave.

Six compiled gameplay regressions cover Ensoul Artifact's animation and retained
mana ability, Ghoulflesh's pump and added color/type, Indigo Faerie's expiration
for colored/multicolored/colorless targets, Rise from the Grave's new zone identity
and subsequent reset on return to hand, Hivestone's controller filter and tribal
bonus, and Angelic Armaments' combined pump/flying/color/type modifiers. Aura and
Equipment removal restore the affected creatures' original characteristics.

Validation: targeted compiled tests; `cargo test --workspace --quiet` including
the replay/regression corpus; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all --check`; and `git diff --check`. Workspace tests run outside
the sandbox for their existing loopback networking tests. Changes remain uncommitted.
