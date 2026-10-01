# Validate custom object-only combat damage

Non-default DamageAssignment answers now require distinct offered blockers,
exactly the available damage, and lethal damage on earlier blockers before
later blockers receive damage, matching the engine's current ordering model.
Totals use u64 so malicious u32 amounts cannot overflow validation. Rejected
answers restore the pending question without settling damage.

A combat regression rejects duplicate rows, an unrelated attacker target,
overflow-sized totals, a short total, and skipping the first blocker. It verifies
the same question survives each rejection, then accepts assigning all damage to
the first blocker and checks only one blocker dies. Trusted defaults continue
to recover canonical player trample damage.

Limitation: the object-only custom answer cannot explicitly allocate player
trample damage; only canonical defaults currently retain player damage. A full
custom editor requires a target-capable answer representation or explicit excess
allocation semantics.

Validation: all 26 combat tests, engine library/combat-test Clippy with dependency
lints disabled and warnings denied, and diff checks pass. Source formatted.
Changes remain uncommitted; concurrent engine edits were preserved.
