# Keep blocker controls synchronized

Assigning or toggling a block through the question panel now clears a pending
battlefield selection for that same blocker. Previously a completed blocker
could remain pending, allowing a later battlefield attacker click to reassign it.
A different pending blocker is retained when editing another block.

The regression checks both selection cases, prevents stale attacker clicks from
changing the completed block, and verifies reassignment and deselection.

Validation: all 15 UI tests and app library/binary Clippy with dependency lints
disabled and warnings denied pass. Source formatted directly; diff checks pass.
Changes remain uncommitted. Other workers' files were preserved.
