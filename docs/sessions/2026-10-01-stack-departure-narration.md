# Avoid claiming countered spells resolved

A stack object disappearing without a corresponding new permanent is now
reported as "left the stack". Projected views cannot distinguish a countered
spell from an instant/sorcery that resolved into the graveyard, so the previous
"resolved" wording asserted an outcome the UI did not know. Spells that enter
as permanents retain their existing battlefield-entry narration.

A regression covers a creature spell leaving the stack for the graveyard without
entering the battlefield. Code edits are confined to narrate.rs; engine/compiler
work was preserved.

Validation: all 13 narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
