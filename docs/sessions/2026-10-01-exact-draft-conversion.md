# Reject lossy draft-to-match conversion

Draft::spec now returns Result rather than silently capping counts at 255 and
dropping cards that cannot be read. Main and sideboard counts must fit the
portable deck representation, and every card and commander must exist in the
active database. Database errors are propagated as useful messages.

Playtest reports conversion failures in the builder instead of starting a
different deck. Validation also reports representation limits. A regression
covers the 255/256 boundary, main and sideboard limits, and missing main/commander
cards. All workspace callers were updated for the Result return type.

Validation: all 30 builder tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Changes remain uncommitted.
