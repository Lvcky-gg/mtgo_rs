# Choose blocker order

OrderBlockers questions now show named blockers with move-up and move-down
controls and explicit confirmation. The answer preserves the player's top-to-
bottom order as indices into the engine's blocker list. The ordering resets
between questions; no blocker can be added or removed through these controls.

Previously the question used Continue and the policy's default order.

Validation: GUI build and app all-target Clippy with dependency lints disabled
and warnings denied pass. Changes remain uncommitted. Interactive visual
inspection was not performed.
