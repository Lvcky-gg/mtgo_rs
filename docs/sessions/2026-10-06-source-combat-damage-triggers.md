# Source combat damage triggers

Added self-trigger phrase “is dealt combat damage,” using the existing
TakesDamage pattern with combat_only true. Existing event amount and subject
bindings retained. No engine/IR changes.

Frozen comparison: 15,934 → 15,937 / 34,913 (45.6%), three gains, zero losses.
New supported cards: Pious Warrior, Wall of Essence, Wall of Souls. Baseline
includes concurrent Claude changes since the last batch.
Snapshot /tmp/mtgo-source-combat-damage-comparison; comparison outputs:
/tmp/mtgo-source-combat-damage-before.txt and /tmp/mtgo-source-combat-damage-after.txt.

New parameterized gameplay test in compiled/generic_damage_triggers.rs verifies
Wall of Essence gains three life when blocking a 3/3 and gains no life from a
three-damage spell. Oracle suite: 849 passed, 15 existing ignored.
Oracle all-target Clippy passed.
