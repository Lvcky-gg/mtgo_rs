# Standalone hand reveals

Added player verb support for revealing a whole hand, using the existing public Reveal effect and hand ownership selector. Supports target players/opponents, each player/opponent, and the caster, including compound verbs such as losing life and revealing the same player's hand.

Two gameplay regressions cover empty and populated targeted hands, shared targets with life loss, all-player and caster-only reveals, and cards staying in their owners' hands.

Validation: engine and Oracle tests passed (1,247 passed, 38 ignored); workspace Clippy with all targets/features and warnings denied passed; formatting check passed. Live database coverage: 16,508 of 34,913 cards playable (47.3%). The change from the prior 16,478 snapshot includes concurrent work and is not attributed solely to this parser addition.

Changes remain uncommitted.
