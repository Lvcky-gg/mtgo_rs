# Enduring, and "a creature type other than Wall"

Claude, card coverage round 38. 16,562 → 16,571 / 34,913 (47.5%): 8 faces from this
round, 1 Codex's. Zero faces lost against HEAD and round 37.

- "When ~ dies, if it was a creature, …": on a self `Dies` trigger the condition always
  holds (only creatures die), so the trigger compiler drops it.
- "… return it to the battlefield under its owner's control. It's an enchantment.": a
  follow-up in `clauses::effect` after a `MoveZone` to the battlefield — a permanent
  `SetTypes([Enchantment])` on `Binding::It`, which `MoveZone` binds to the returned
  object's new identity.
- `Effect::Choose` gained `except: Option<Subtype>` (serde default): "Choose a creature
  type other than Wall. … becomes that type …"; the excluded type isn't offered.

Tests: `enduring.rs` (destroyed, it returns an enchantment and not a creature),
`choose_on_resolution.rs` (Wall isn't among the options; the target becomes an Elf).
