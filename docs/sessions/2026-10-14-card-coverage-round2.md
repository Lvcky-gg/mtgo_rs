# Card coverage, round 2 — 2026-10-14

Against the same `/tmp/card-coverage.sqlite` snapshot, coverage rises from
17,597 to 17,616 / 34,914 (50.5%). A complete card-name comparison gains 19
cards with zero losses. The existing five-card lookup difference remains:
the name-check command reports 17,621 playable cards.

## Shared player predicates

Player actions accept comma-separated verbs and the final `, and` conjunction,
retaining the original subject and target slot. Parsing a continuation that
belongs to another subject restores the compiler context before falling back
to the general effect sequence. Actions resolve in printed order.

Newly playable: Archon of Cruelty; Atrocious Experiment; Caress of Phyrexia;
Decimator Web; Demogorgon's Clutches; Funeral Rites; Greed's Gambit; Mind Drain;
Pointed Discussion; Scarring Memories; Smallpox.

## Combined continuous modifications

The modification parser accepts a following predicate with the same subject,
including keyword gains and losses in either order. All modifications share
the printed duration and use existing engine behavior.

Newly playable: Canopy Dragon; Fear of Falling; Gargoyle Sentinel; Jaded Analyst;
Leering Gargoyle; Sky Tether; Wind Shear; Xathrid Slyblade.

Seven compiled gameplay regressions cover Archon of Cruelty's distinct recipients,
Mind Drain's single opponent target, Smallpox with and without opposing sacrifice
candidates, temporary gains/losses in both orders, removal of Sky Tether, and
Wind Shear's selection of attacking flyers. Wind Shear retains its power/toughness
penalty after flying is removed; both modifications expire at cleanup.

Validation: targeted compiled tests; `cargo test --workspace --quiet` including
the replay/regression corpus; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all --check`; and `git diff --check`. Workspace tests run outside
the sandbox for their existing loopback networking tests. Changes remain uncommitted.
