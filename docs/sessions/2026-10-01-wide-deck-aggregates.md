# Preserve large deck aggregates

Draft main and sideboard totals, mana-curve buckets, and combined per-card badges
now use u64 aggregates over u32 row counts. Multiple valid rows can therefore
sum above u32::MAX without panicking in debug builds or wrapping in release.
The chart and badge rendering accept the wider counts.

A regression checks large totals, two spell rows in the same curve bucket,
land exclusion, and a main/side/commander combined badge.

Validation: all 33 builder tests, app library/binary Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Source formatted directly.
Changes remain uncommitted.
