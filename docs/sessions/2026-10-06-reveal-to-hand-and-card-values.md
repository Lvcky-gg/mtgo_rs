# Reveal to hand and moved-card values — 2026-10-06

Added the printed “reveal the top card ... and put that card/it into your hand”
shape using existing Reveal and MoveZone primitives. The compiler retains the
moved card through Binding::It for following instructions. value_phrase also
accepts “that card's” alongside its existing creature and pronoun references.

A Reanimate gameplay regression exposed stale object context after putting a
card onto the battlefield. Singular put/return operations now preserve the new
object identity for subsequent instructions, including returns to hand. This
matches the existing resolver's MoveZone binding behavior; no engine/IR changes.

Frozen-source comparison: 15,804 → 15,820 / 34,913 (45.3%), +16, zero lost.
Added Augury Adept, Dark Confidant, Dark Tutelage, Darkstar Augur, Doctor Jane
Foster, Fated Return, Grave Upheaval, Lie in Wait, Mirkwood Elk, Phyrexian Delver,
Pyretic Rebirth, Razor Hippogriff, Reanimate, Rotfeaster Maggot, Surprise Deployment,
Volcanic Vision. Claude's concurrent changes were held constant in the comparison.

Five regressions cover revealed-card identity and reveal-before-move ordering,
empty-library behavior, complete printed upkeep triggers, Reanimate taking an
opponent's creature and losing the correct life, and returning an artifact to
hand before gaining its mana value in life. Pain Seer's untap trigger remains
unsupported; it was not included in the measured gains.

Validation: Oracle suite 835 passed, 15 existing ignored; Clippy with warnings
denied and diff check pass. An unrelated concurrent shared-exile test assertion
was corrected by Claude before the final passing run.

Changes: new reveal shape and registration, value_phrase card alias,
put_onto_battlefield/bounce moved-object context, compiled/reveal_hand.rs plus
registration, README and coordination documentation.
