# Card coverage, round 6 — 2026-10-14

Against the same `/tmp/card-coverage.sqlite` snapshot, coverage rises from 17,662
to 17,678 / 34,914 (50.6%). The complete card-name comparison gains 16 cards
with zero losses. The existing five-card name-lookup difference remains
(17,683 versus 17,678).

## Graveyard returns chosen during resolution

The return parser accepts a single qualifying card from your graveyard without
targeting it. A mandatory return must choose an available matching card; an empty
candidate set does not stop subsequent effects. Choices happen during resolution,
so cards just milled by the preceding instruction are available. Optional returns
retain the existing confirmation prompt.

Player verb sequences also accept `you return`, allowing follow-up draws and life
gain to retain their subject. Witherbloom Command mills the announced player's
library while returning a card from its controller's graveyard.

Newly playable: Acolyte of Affliction; Carrion Cruiser; Corpse Churn;
Cruel Ultimatum; Dig Up the Body; Eccentric Farmer; Fell Gravship;
Grapple with the Past; Inspiration from Beyond; Pothole Mole;
Takenuma, Abandoned Mire; Witch of the Moors; Witherbloom Command.

## Source type and exile ownership conditions

Conditions can test whether the source is currently a permanent card type,
including a creature. Flywheel Racer's mana ability requires it to be a creature;
Foriysian Totem's additional blocking allowance requires its animated form.
`you own a card in exile` checks ownership rather than control.

Newly playable: Dreadlight Monstrosity; Flywheel Racer; Foriysian Totem.

Seven compiled gameplay regressions cover Cruel Ultimatum's complete sequence and
filtered mandatory choice, its empty-graveyard case, optional returns of freshly
milled creatures, Flywheel Racer's crewing and expiration, Witherbloom Command's
distinct recipients and mode targets, Takenuma's Channel discard/mill/return,
and the exile condition with no card, an owned card, or another player's card.

Validation: targeted compiled tests; `cargo test --workspace --quiet` including
the replay/regression corpus; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all --check`; and `git diff --check`. Workspace tests run outside
the sandbox for their existing loopback networking tests. Changes remain uncommitted.
