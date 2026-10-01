# Narrate permanent counter changes

The match log now reports counters added to or removed from objects that remain
on the battlefield, using readable counter names and resulting totals. Missing
counter entries count as zero; arithmetic uses i64 to safely compare totals.

A regression covers adding and removing multiple +1/+1 and stun counters,
deterministic ordering, and unchanged views producing no messages. Newly entering
objects retain their existing battlefield-entry narration.

Code edits are confined to narrate.rs; engine/compiler work was preserved.

Validation: all 16 narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
