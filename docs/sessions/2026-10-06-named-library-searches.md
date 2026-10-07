# Named library searches — 2026-10-06

Library searches now recognize specified printed names and single-card name
alternatives. ObjectFilter::Named stores a normalized name; evaluation and trigger
filter detection compare it with the object's current printed face name. IR walks
handle the new filter as a leaf. Existing NamedLikeSource remains source-relative.

The parser preserves commas inside names and separates a fixed-name alternative
from a repeated noun, such as “Halvar, God of Battle or an Equipment card”.
Separate card quotas and partial normalized self-references remain rejected.

Frozen-source comparison: 15,660 → 15,664 / 34,913 (44.9%), +4, zero lost.
Added Bogbrew Witch, Dragonstorm Forecaster, Forging the Tyrite Sword, Nissa Revane.
Claude's concurrent changes were held constant in the comparison.

Four regressions cover choosing either named alternative or none, tapped entry,
exact names versus near matches, comma-containing names, type alternatives,
source-name searches, and unsupported quotas. The fixture harness now supports
explicit printed names while preserving generated names for existing fixtures.

Validation: 1,068 tests passed, 38 existing ignored across mtg-ir, mtg-engine,
and mtg-oracle; Clippy with warnings denied passed. GUI build and diff check passed.

Changes: mtg-ir selector/walk leaf, mtg-engine eval/detect match arms,
clauses::search_quality only, named_search tests, harness name helper,
module registration, README and coordination documentation.
