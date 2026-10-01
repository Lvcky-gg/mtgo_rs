# Preserve counts when loading saved decks for play

The saved-deck match conversion now rejects counts above 255 instead of silently
reducing them. The saved list remains unchanged, and callers receive a clear
representation-limit error. This closes the same loss of fidelity fixed in
Draft::spec for builder playtests.

A regression covers the accepted 255-copy boundary and rejects 256 copies in
both main and sideboard sections while verifying the stored count remains 256.

Validation: all four deck tests and app all-target Clippy with dependency lints
disabled and warnings denied pass. Changes remain uncommitted.
