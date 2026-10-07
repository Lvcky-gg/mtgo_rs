# Owner-targeted shuffles

The compiler accepts "Choose target artifact or enchantment. Its owner shuffles
it into their library", and "The owner of target [object] shuffles it into their
library". This covers Deglamer, Unravel the Aether, and Cathartic Parting's complete
printed text when combined with the selected-graveyard shuffle grammar.

The object moves with Natural placement; its new identity is bound as It, whose
owner is then used for the shuffle. No engine or IR changes. The pronoun form
requires a known single target or self source rather than guessing a plural group.
Remaining longer effects such as Chaos Warp's reveal/permanent follow-up are not
claimed as supported.

Three gameplay regressions verify owner versus controller, exactly one shuffle of
the correct library, Cathartic Parting's optional own-graveyard follow-up using a
separate library, and no shuffle if the target is destroyed in response. Full
engine/Oracle validation passes 1,181 tests with 38 existing ignored. All-target
Clippy with warnings denied and diff whitespace checks pass. The exert regression
that failed last session now passes. Changes remain uncommitted.
