# Surface deck-builder search errors

Database search failures now display their error and a Retry search control.
They clear stale cards and pagination availability instead of appearing as an
empty result or triggering the missing-legality explanation. Failed requests
are cached until retry or a query change, preventing per-frame database retries.
Successful retry clears the error and restores results.

The regression temporarily renames the cards table in an in-memory database,
checks stale-result removal and cached request state, restores the table, and
verifies recovery.

Validation: all 36 builder tests, app library/binary Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Source formatted directly.
Changes remain uncommitted.
