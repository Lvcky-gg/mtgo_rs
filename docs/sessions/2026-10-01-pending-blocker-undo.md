# Undo pending blocker selections first

A blocker awaiting an attacker now counts as an unfinished selection, so the Undo
button is available. Undo clears that newest selection before removing a completed
block or submitting an engine undo. This prevents an older block from disappearing
while the pending blocker remains selected.

A headless UI regression declares one block, selects another pending blocker,
and clicks Undo three times. It checks pending selection removal first, completed
block removal second, and an authorized engine undo only after both are gone.

Validation: all 11 UI tests, app all-target Clippy with dependency lints disabled
and warnings denied, and diff checks pass. Concurrent policy refactoring emitted
dependency warnings; those files were preserved. Changes remain uncommitted.
