# Follow edited decks across rename saves

The builder screen records the draft's previous saved ID before rendering save
controls and its new ID afterward. A successful save remaps player, bot, and
detail selections using the previous ID to the new ID before refreshing.
Selections now survive a rename that changes both name and database ID, without
requiring a name match or changing unrelated choices.

A regression covers remapping the previous deck, retaining unrelated and demo
choices, a new draft without a previous ID, and saves that retain the same ID.
The BuilderAction API is unchanged.

Validation: all five app event/selection/transfer tests, GUI build, app all-target
Clippy with dependency lints disabled and warnings denied, and diff checks pass.
Changes remain uncommitted.
