# Preserve sideboarding copies at entry-count boundaries

Sideboard transfers now choose a positive source count and a destination entry
that can accept another copy. If every matching destination entry is already 255,
the transfer adds another counted entry instead of overflowing u8. Zero-count
source rows are skipped rather than underflowing.

A regression verifies transfer into a full entry, transfer back, total-card
identity preservation, skipping an empty duplicate source, and no transfer when
only empty sources remain. Existing deck-size validation still controls when
sideboarding can finish.

All three app event/transfer tests, app Clippy with dependency lints disabled and
warnings denied, and diff checks pass. Changes remain uncommitted.
