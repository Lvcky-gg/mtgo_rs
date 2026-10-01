# Avoid overflow while narrating life changes

Life-change narration now converts both i32 totals to i64 before subtracting.
The log can safely describe a delta larger than i32::MAX, including changes
between the minimum and maximum representable life totals. Ordinary wording and
game state are unchanged.

A regression covers both directions across the entire i32 range. Code edits are
confined to narrate.rs; concurrent engine/compiler work was preserved.

Validation: all ten narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
