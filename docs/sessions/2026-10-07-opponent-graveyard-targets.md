# Opponent graveyard targets

Added card noun support for “from an opponent's graveyard” and “in an opponent's graveyard”, including typed cards. Both zone recognition and the ownership qualifier use the existing opponent selector.

Gameplay regressions verify legal targets exclude the caster's cards and cards in other zones, preserve creature-type restrictions, exile the selected card, and reanimate under the caster's control while preserving opponent ownership.

Validation: engine and Oracle tests passed (1,249 passed, 38 ignored); workspace all-target/all-feature Clippy with warnings denied and formatting check passed. Live coverage: 16,517 / 34,913 (47.3%), up nine from the preceding snapshot; concurrent work may contribute to this total. Changes remain uncommitted.
