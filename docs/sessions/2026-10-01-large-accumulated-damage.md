# Preserve lethal state for large accumulated damage

Ordinary marked damage now saturates at u32::MAX instead of overflowing during
event application. Each original damage event retains its full amount in the
log. The state-based lethal check compares marked damage and toughness as i64,
so large unsigned damage cannot become negative and incorrectly spare a creature.

Two regressions reproduced an accumulation panic and a missing lethal-damage
state-based action before the fix. They now check saturation with an unchanged
logged event and destruction for u32::MAX marked damage.

Validation: all 32 combat tests and the rules integration suite pass. Engine
library/combat-test Clippy with dependency lints disabled and warnings denied
and diff checks pass. No commit created; follow-up changes remain uncommitted.
