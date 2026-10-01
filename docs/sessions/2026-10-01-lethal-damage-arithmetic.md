# Preserve large marked damage in lethal calculations

The lethal-damage helper now subtracts marked u32 damage from signed toughness
using i64 arithmetic. Casting marked damage to i32 previously made values above
i32::MAX negative, causing a subtraction panic or an incorrect lethal threshold.
The result still clamps at zero.

The regression checks unmarked, partly damaged, lethal, and large marked-damage
values on a 2-toughness creature. It reproduced the subtraction overflow before
the fix and passes after it.

Validation: all 27 combat tests, engine library/combat-test Clippy with dependency
lints disabled and warnings denied, and diff checks pass. Changes uncommitted.
