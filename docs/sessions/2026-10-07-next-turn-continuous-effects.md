# Until your next turn

Temporary boosts and keyword grants now parse the trailing “until your next turn” duration. Existing UntilYourNextTurn continuous effects previously never expired; begin_untap now emits ContinuousEffectEnded for effects whose controller is the active player before evaluating untap restrictions or optional untapping. Ownership of the affected object does not determine expiration.

Two gameplay regressions check a boost and flying on an opponent-owned creature survives that opponent's turn and expires on the caster's next upkeep, and a group indestructible grant expires while printed flying remains.

Validation: engine and Oracle tests passed (1,256 passed, 38 ignored), workspace all-target/all-feature Clippy with warnings denied passed, and formatting check passed. Live coverage: 16,539 of 34,913 cards (47.4%), up seven from the preceding snapshot; concurrent work may contribute. Changes remain uncommitted.
