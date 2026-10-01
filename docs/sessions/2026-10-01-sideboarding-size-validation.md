# Retain Constructed size limits after sideboarding

Added shared sideboard size validation: the main deck must retain at least 60
cards, and the sideboard must contain at most 15. Local seat redistribution and
host acceptance of remote sideboarding both enforce these limits alongside the
existing card-conservation check. Invalid redistribution retains the previous
deck, matching existing handling of changed card collections.

The UI disables Done while these limits are violated and displays an explanation,
allowing temporary invalid sizes while moving cards. A regression verifies a
valid 60/15 redistribution and rejected sizes while conserving the same 75 cards.

Code edits are in mtg-session matches.rs and mtg-app app.rs; engine/compiler work
was preserved. No GUI visual inspection was performed.

The regression also independently rejects a 59-card main deck with only one card
in the sideboard. Final validation: all six match tests, app compilation,
session/app all-target Clippy with dependency lints disabled and warnings denied,
and diff checks pass.
