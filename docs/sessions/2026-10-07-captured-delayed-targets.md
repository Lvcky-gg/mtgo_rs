# Captured targets for delayed effects

Simple delayed destruction, zone movement, tap, and untap actions can now refer
to original spell targets. The compiler captures the selected Target or Union
of Target selectors with Effect::Let before creating Effect::Delayed. The engine
already saves the binding snapshot when registering the delayed trigger, so the
later action has no target of its own and retains the original object identity.
The binding uses Named(u16::MAX - 8), separate from existing parser bindings.
Complex remaining Target references continue to be rejected.

Three regressions verify that the action waits until the specified step, later
hexproof does not stop a nontargeted delayed action, another creature is untouched,
a permanent that leaves and returns has a new identity and survives, and an
illegal original target creates no delayed action.

Validation: 1229 engine/Oracle tests passed, 38 ignored. Workspace
all-target/all-feature Clippy and formatting pass. The database audit reports
16,454 / 34,913 cards playable as printed (47.1%), versus the previous reported
16,432. Concurrent coverage work can contribute to this total.
