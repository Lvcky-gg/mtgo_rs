# Bot hand management

Codex contribution alongside Claude's bot search work. `Bot::worst_cards` used
`lands > 3` to rank every land as surplus. A multi-card discard or London
mulligan bottom could therefore give up necessary lands before expensive spells.
It now marks exactly `land_count - 3` lands as surplus, using stable object IDs,
then ranks expensive spells, then the remaining lands.

A regression through `Bot::decide` covers four lands/discard two and five
lands/discard three, with reversed candidate order. Both keep three lands and
the cheap spell, giving up surplus lands and the seven-drop.

Validation: 48 policy tests pass; headless bot gameplay integration passes;
package formatting and diff checks pass. This retains the existing fixed
three-land heuristic; it does not estimate colored mana or a deck's mana curve.
Changes are in `crates/mtg-policy/src/bot/mod.rs`; existing work retained.
