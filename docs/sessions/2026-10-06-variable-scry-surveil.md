# Variable scry and surveil — 2026-10-06

Scry/surveil now accept existing Value amounts, including mana X and explicitly
defined X. replace_x visits LookAndSort's count, so defined counts reach runtime
instead of leaving X unresolved. Undefined X remains rejected.

The definition preprocessor also accepts an inline scry definition followed by a
fixed-number draw, as on Ugin's Insight. That continuation is limited to a draw
with no remaining text; later references to X remain rejected in this form.

Frozen-source comparison: 15,768 → 15,772 / 34,913 (45.2%), +4, zero lost.
Added Lydia Frye, Oath of Jace, Siani, Eye of the Storm, Ugin's Insight.
Claude's concurrent changes were held constant in the comparison.

Three regressions verify a greatest-mana-value scry followed by drawing three,
a surveil count restricted to matching tapped creatures, mana-X compilation,
and rejection of undefined X and unsupported continuations. The tapped fixture
is set after the game's normal untap step.

Validation: Oracle suite 826 passed, 15 existing ignored; Clippy with
warnings denied and diff check pass. No IR or engine changes needed.

Files: clauses::scry_surveil, replace_x's LookAndSort arm, the effect-definition
continuation guard, compiled/variable_scry.rs plus registration, README and docs.
