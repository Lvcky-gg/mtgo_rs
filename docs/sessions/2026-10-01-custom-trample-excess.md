# Preserve custom trample excess without changing the wire shape

Custom DamageAssignment rows may now leave damage for the known attack destination
when the attacker has trample and every blocker has lethal damage assigned.
The engine appends that excess to the settled assignment. Short totals without
trample, or with a nonlethal blocker, remain rejected. Over-allocation, duplicates,
unrelated targets, and invalid blocker ordering retain their validation.

Answer::DamageAssignment documents these excess semantics. No serialized variant
or payload shape changed. This resolves the custom-player-trample limitation
recorded in earlier combat handoffs; a GUI allocation editor remains future work.

The regression rejects leaving one blocker unassigned while preserving the
question, then accepts a non-default 2/1 blocker split from four total damage,
kills both blockers, and verifies exactly one damage reaches the player.

Validation: all 30 combat tests, engine library/combat-test Clippy with dependency
lints disabled and warnings denied, and diff checks pass. Source formatted.
Changes remain uncommitted.
