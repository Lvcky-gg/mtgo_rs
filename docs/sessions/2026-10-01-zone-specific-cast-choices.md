# Preserve casts from different zones

Priority action deduplication now includes the source object's ZoneRef, so the
same printed card castable from hand and graveyard/exile/library remains two
choices. Identical copies within the same zone still collapse, and different
faces remain separate. Non-play actions remain unchanged. The original public
deduplication helper remains available.

Play labels now identify graveyard, exile, library, and command-zone origins.
The existing adventure origin label takes precedence over generic exile.

A regression covers two identical copies in hand and a third in the graveyard.
Code edits are in board.rs and ui.rs; engine/compiler work was preserved.
No GUI visual inspection was performed.

Validation: all 21 board tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
