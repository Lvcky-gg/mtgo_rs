# Library reordering — 2026-10-06

ReorderLibraryTop now compiles for targeted player/opponent libraries, mana X,
and existing inline own-library count definitions. The resolver asks the resolving
controller to choose the order, rather than the library owner. Existing fixed-count
own-library reordering retains its behavior. No IR or view changes were needed.

Frozen-source comparison: 15,744 → 15,750 / 34,913 (45.1%), +6, zero losses.
Added Architects of Will, Descendant of Soramaro, Elemental Augury,
Information Dealer, Second Sight, Soothsaying. Claude's concurrent changes were
held constant in the comparison.

Three regressions verify caster-owned choices for an opposing library, private
visibility only during those choices, unchanged cards below the looked-at group,
inline hand-size counts, mana-X compilation, and rejection of undefined X.

Validation: 1091 tests passed, 38 existing ignored across mtg-ir,
mtg-engine, and mtg-oracle. Clippy with warnings denied and diff check pass.

Changes confined to clauses::reorder_top, the ReorderLibraryTop resolver's chooser,
compiled/reorder_library.rs plus registration, README and coordination docs.
