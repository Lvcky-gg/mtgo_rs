# Keep one looked-at card on top — 2026-10-06

Numeric library selections now accept “on top of your library” and “back on top
of your library” for one selected card. The Dig resolver supplies index zero for
its Library take destination; the rest retain their printed destination. Existing
hand selections retain their behavior, with explicit “up to” numeric quotas also
accepted. No new IR fields were needed; the Dig documentation describes the
Library destination convention.

Frozen-source comparison: 15,736 → 15,740 / 34,913 (45.1%), +4, zero lost.
Added Gurmag Nightwatch, Gutless Plunderer, Sage of Days, The Mana Rig.
Claude's concurrent changes were held constant in the comparison.

Three regressions exercise keeping each possible card or none in Sage of Days's
entry trigger, declining or taking an explicit up-to-one choice with a bottom
remainder, and rejection of unsupported multiple-card top ordering. They verify
that the untouched portion of the library stays directly below the kept card.

Validation: 1087 tests passed, 38 existing ignored across mtg-ir,
mtg-engine and mtg-oracle. Clippy with warnings denied and diff check pass.

Changes confined to clauses::dig's numeric branch, Dig resolver destination index,
Dig's IR documentation, compiled/keep_top.rs and module registration, README
and coordination documentation.
