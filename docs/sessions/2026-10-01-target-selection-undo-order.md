# Undo targets in selection order

Target selection now records the order in which slots are selected. Undo removes
the most recently selected target, including when slots were chosen out of order.
Deselecting a target removes its slot from that history, and clearing the question
clears the history as well.

A headless UI regression selects slot 1 before slot 0, verifies Undo removes slot
0 first, then checks explicit deselection and the remaining Undo operation.

Validation: all 12 UI tests, app all-target Clippy with dependency lints disabled
and warnings denied, and diff checks pass. Concurrent policy changes were left
untouched. The implementation is present in the current repository HEAD following
another worker's commit; this handoff was added separately.
