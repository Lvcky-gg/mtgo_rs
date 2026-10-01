# Validate and clarify the combat editor

The full app suite passes: 175 library tests, 2 network-match integration tests,
and 15 UI integration tests (192 total). All 30 engine combat tests also pass.
This verifies the custom allocation editor, rejected-answer feedback, validation,
and trample handling together after the repository history cleanup.

The editor now numbers blockers in their assignment order and explicitly shows
unassigned damage or how much an over-allocation must be reduced. This small
presentation improvement preserves existing allocation and validation behavior.
After that change, all 19 UI tests and app all-target Clippy with dependency lints
disabled and warnings denied pass. Formatting and diff checks pass.

No commits were created; the clean root history remains intact. Changes from this
follow-up remain uncommitted.
