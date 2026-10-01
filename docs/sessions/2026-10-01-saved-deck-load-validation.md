# Validate saved deck rows before editing

Draft loading now uses checked addition for duplicate main/sideboard rows and
rejects unsupported commander cardinality. A malformed saved deck can no longer
panic on count overflow or silently lose extra commanders when opened for
editing. Normal duplicate rows still merge correctly.

Import summaries also use checked main/sideboard totals before saving. Lists
whose totals overflow u32 produce an error and preserve an existing same-name
deck. Regressions cover duplicate merging, overflow, commander counts/rows, and
oversized imported totals in both sections.

Validation: all 31 builder tests, all five deck tests, and app all-target Clippy
with dependency lints disabled and warnings denied pass. Final integrated
validation is recorded in the autonomous progress handoff.
