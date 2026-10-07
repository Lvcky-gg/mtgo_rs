# Tapped library selection — 2026-10-06

Implemented printed “onto the battlefield tapped” in library selection. Effect::Dig
carries a serde-defaulted tapped flag; the compiler preserves the modifier, and the
resolver applies the existing tapped-entry event after normal entry processing.

Frozen-source comparison: 15,621 → 15,624 / 34,913 (44.8%), +3, zero losses.
Newly supported: Cartographer's Survey, Elvish Rejuvenator, Silverback Elder.
Claude's concurrent compiler changes were held constant in the comparison.

Three gameplay tests exercise zero/one/two land selections, revealed selection
with graveyard remainder, and ordinary untapped entry. Updated the old negative
tapped-entry assertion to reject tapped-and-attacking entry, which remains unsupported.
Separate card-type quotas also remain unsupported.

Validation: 1057 tests passed, 38 existing ignored across mtg-ir,
mtg-engine, and mtg-oracle; Clippy with warnings denied, GUI build, and diff check passed.

Files: mtg-ir/src/effect.rs (Dig only), mtg-oracle/src/compile/clauses.rs (dig only),
mtg-engine/src/resolve.rs (Dig only), compiled/tapped_dig.rs and module registration,
compiled/dig.rs negative regression, README.md and coordination documentation.
