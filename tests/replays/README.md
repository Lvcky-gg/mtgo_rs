These are synthetic engine recordings, not recorded alpha games or external
oracles. Each JSON contains embedded card IR, an initial state, ordered answers,
and canonical state/digest checkpoints after every answer.

The basic casting, two-spell stack, and enters-trigger fixtures also include
independently specified life totals. Campaign-derived snapshots provide
deterministic regression evidence, not independent rules truth.

Regenerate only during intentional fixture creation or version migration:
`cargo run -p mtg-verify --example seed_corpus`.
Ordinary CI runs `cargo test -p mtg-verify --test corpus` and never regenerates.

Long-game, storm, counter-war, replacement, and real player-game
recordings are not yet represented. Do not count these synthetic recordings as
coverage for those scenarios.

The synthetic Commander fixture exercises casting from the shared command zone
in a four-player 40-life setup. It is not a complete Commander game and does not
verify commander-damage elimination, deck construction, or every Commander
interaction. A separate owner-declines-return replay exercises the optional
graveyard return and subsequent priority without a repeated prompt.
