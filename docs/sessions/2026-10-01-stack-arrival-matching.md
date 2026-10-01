# Avoid unsupported stack-to-battlefield matches

Stack departures are suppressed in favor of battlefield-entry narration only
for spells with a known card ID matching a new permanent. Two missing card IDs
no longer count as evidence of a match, and abilities cannot become permanents
merely because an arrival shares their source card ID.

Two regressions cover hidden objects and abilities. Existing known-creature
resolution coverage still passes. Same-card simultaneous arrivals remain
ambiguous because projected views do not expose cross-zone object lineage.

Validation: all 20 narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Changes are uncommitted.
