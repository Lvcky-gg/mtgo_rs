# Untap and goad

Added compound “untap and goad” parsing with a shared object selector. Leading “until end of turn, gain control” now sets temporary control and carries that duration to a following keyword grant within the same sentence.

Gameplay regressions verify full temporary theft plus haste, untapping, and goad text; control and haste expire at cleanup while goad remains through the opponent's turn and expires at the caster's next turn. A second regression checks compound target effects only affect the selected creature.

Validation: engine and Oracle tests passed (1,259 passed, 38 ignored); workspace all-target/all-feature Clippy with warnings denied passed; formatting check passed. Live coverage: 16,546 of 34,913 cards (47.4%), up seven from the preceding snapshot; concurrent changes may contribute. Changes remain uncommitted.
