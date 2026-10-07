# Repeated search alternatives — 2026-10-06

Single-card library searches now accept repeated nouns separated by “or a” or
“or an”. Each branch keeps its adjectives and numeric or keyword restrictions.
The existing selector asks for at most one card across the alternatives.
Separate quotas (“an instant card and/or a sorcery card”) remain rejected.

Frozen-source comparison: 15,624 → 15,632 / 34,913 (44.8%), +8 cards, zero lost.
Nine faces gained: Gatecreeper Vine, Mystical Teachings, Outcaster Greenblade,
Shefet Monitor, Silver Deputy, Spinewoods Armadillo, Starfield Shepherd,
Waterlogged Teachings, and its reverse face Inundated Archive.
Claude's concurrent changes were held constant in the comparison.

Four regressions cover optional selection, basic/nonbasic subtype alternatives,
instant/flash alternatives, independent mana-value restrictions, and rejection
of separate quotas. Oracle suite: 791 passed, 15 existing ignored;
final targeted four-test run and Clippy with warnings denied also pass.

Changes confined to clauses::search's alternative loop, new
compiled/search_alternatives.rs, test module registration, README and docs.
