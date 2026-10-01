# Bound sample-hand library expansion

Draft::shuffled now returns a Result and rejects main libraries above 10,000
cards before expanding copies or allocating a shuffle permutation. This bounds
practice allocations for decks with huge imported counts. The builder displays
the error; opening, redraw, and mulligan paths handle the Result. Stored decks
and their counts are retained.

The regression rejects u32::MAX copies, accepts exactly 10,000 cards, ignores a
huge sideboard, and rejects one main-deck card above the limit.

Source formatted directly; app library/binary Clippy with dependency lints
disabled and warnings denied and diff checks pass. Initial tests encountered
transient concurrent engine edits; builder tests were retried after those edits
were corrected. Changes remain uncommitted.

Retried validation: all 34 builder tests pass.
