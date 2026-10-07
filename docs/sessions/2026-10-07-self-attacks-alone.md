# Self attacks-alone triggers

The compiler recognizes “whenever this creature attacks alone”. AttackingAlone is
an object filter implemented in both normal evaluation and trigger detection, and
visited as a leaf by the IR walker. It matches only when the object is the sole
attacking creature. Self-reference parsing places this phrase before “attacks”.

Generic attacks-alone triggers and exalted now use this event filter rather than
an intervening-if condition. An attacker declared alone triggers even if another
creature is put attacking before the trigger resolves. Explicit intervening-if
conditions on a printed ability still retain their normal checks.

Four gameplay regressions cover solo and multiple attackers, pump damage, Food
creation only on the source's own attack, generic control-scoped triggers, and
both exalted and a self trigger resolving after a second creature joins combat.

Validation: 1237 engine/Oracle/IR tests passed, 38 ignored. Workspace
all-target/all-feature Clippy and formatting pass. Live database audit:
16,475 / 34,913 cards playable (47.2%), compared with the prior 16,454 report.
Concurrent coverage changes can contribute to this total.
