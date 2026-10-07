# Selected graveyard shuffles

The compiler accepts "Shuffle [targeted cards] from your graveyard into your
library", including any-number and optional single-card targets. Renewing Touch
and Piper's Melody use this shape. Selected cards move with Natural placement,
then the library shuffles; no intermediate ordering choice is needed.

The existing whole-graveyard and hand-plus-graveyard shuffle compiler also uses
Natural placement so it no longer asks owners to order cards immediately before
shuffling. Shared target-spec and ownership checks are reused from library
placement. Cards from another owner's graveyard into your library remain rejected.
No engine or IR changes; concurrent mana work was preserved.

Four regressions cover zero/one/several selected creatures, untouched graveyard
cards, actual shuffle events, optional noncreature selection, rejected cross-owner
destinations, and whole-graveyard shuffling without ordering prompts. Full engine
and Oracle validation: 1,170 passed, 38 existing ignored. All-target Clippy with
warnings denied and diff whitespace checks pass. Changes remain uncommitted.
