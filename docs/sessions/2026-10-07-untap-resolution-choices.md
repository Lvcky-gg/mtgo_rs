# Untap choices during resolution

The compiler accepts “untap up to N lands”, including “you control”, using the
existing ChosenBy selector. This is a resolution-time selection and introduces
no target slots. Untap resolution now asks for ChosenBy objects rather than
sending that selector through the evaluator without a choice.

The existing selection helper offers matching battlefield objects across both
players, permits choosing none, filters invalid or duplicate picks, and clamps
selection to the stated maximum. Unrestricted effects can untap an opponent's
lands; restricted effects offer only lands controlled by the resolving player.
Already untapped lands remain valid selections.

Four regressions cover zero/one/two selections, ownership and control boundaries,
invalid and duplicate picks, printed free-spell and creature texts, preservation
of targeted untap compilation, and an actual creature-entry trigger.

Validation: engine and Oracle tests pass (1,194 passed, 38 ignored), formatting
check passes, and all-target engine/Oracle Clippy passes. The live database audit
reports 16,341 / 34,913 cards playable as printed (46.8%), compared with the
previous snapshot of 16,311. Concurrent coverage work can contribute to totals.
