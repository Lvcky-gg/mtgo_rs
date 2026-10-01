# Close play options before engine Undo

An open card play-options menu now counts as a pending selection. Undo closes
that menu before sending an engine Undo, and the Undo button is available for
closing it even when the engine does not permit undoing a game action.

The headless regression checks both engine-undo settings, keeps the question
active after closing the menu, and verifies that a second Undo submits the
authorized engine action.

Validation: all 13 UI tests pass. App library/binary Clippy with dependency lints
disabled and warnings denied passes. The changed source was formatted directly.
Package formatting and all-target Clippy are blocked by the separate untracked
`crates/mtg-app/tests/ui_rendering.rs`: it references a missing common module and
outdated PlayerView fields, among other diagnostics. That file was preserved.
Changes remain uncommitted.
