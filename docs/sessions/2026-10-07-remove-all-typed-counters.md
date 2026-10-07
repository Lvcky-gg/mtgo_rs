# Remove all typed counters

Compiler supports “remove all +1/+1 counters” and other recognized counter kinds from battlefield selectors. Uses ForEach with a bound object and dynamic counter count so each removal records its own exact count, including zero-counter permanents.

Gameplay regressions cover targeted and all-creature removal, uneven counter counts, preserved shield counters, exact event deltas, and source-only end-step removal.

Validation: engine and Oracle tests passed (1,254 passed, 38 ignored), workspace all-target/all-feature Clippy with warnings denied passed, and formatting check passed. Live database coverage: 16,532 of 34,913 (47.4%), up 15 from the preceding snapshot; concurrent work may contribute. Changes remain uncommitted.
