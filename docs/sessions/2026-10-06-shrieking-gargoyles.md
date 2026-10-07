# Tyranid Harridan printed label

Added Shrieking Gargoyles to the explicit strip_ability_word prefix list. This
finishes Tyranid Harridan support after the previous union combat-trigger batch.
Harness subtype list appends Tyranid and Gargoyle. Printed gameplay regression
checks the source and another Tyranid each create a flying 1/1 token from combat
damage to a player. No behavioral engine/IR changes.

Frozen comparison: 15,999 → 16,000 / 34,913 (45.8%), one gain, zero losses:
Tyranid Harridan. Snapshot /tmp/mtgo-shrieking-comparison; outputs
/tmp/mtgo-shrieking-before.txt and /tmp/mtgo-shrieking-after.txt.
Oracle suite: 860 passed, 15 existing ignored.
Removed redundant let mut distinct = distinct in concurrent engine modal cost
work to resolve Clippy's redundant_locals warning; existing binding already mutable.
