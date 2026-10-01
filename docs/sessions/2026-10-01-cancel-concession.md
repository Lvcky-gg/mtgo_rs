# Cancel concession confirmation

The priority controls now offer Cancel concede beside Really concede?, so an
accidental first click can be dismissed while staying on the same game question.
Confirming concession returns immediately after submitting the action.

A headless regression arms the confirmation, cancels it without sending an
answer, then arms it again and confirms the concession action.

Validation: all 14 UI tests, app library/binary Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Source formatted directly.
The separate unfinished integration test remains untouched. Changes uncommitted.
