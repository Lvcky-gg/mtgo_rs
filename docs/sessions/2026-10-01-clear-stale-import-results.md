# Clear stale import results while editing

Changing the import name or pasted list now clears the previous import result.
Unmatched/unreadable warnings and saved summaries no longer describe input that
has already been corrected or renamed. Editing retains the input itself.

Validation: GUI build, app all-target Clippy with dependency lints disabled and
warnings denied, and diff checks pass. Changes remain uncommitted.
