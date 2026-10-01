# Match death narration against new graveyard arrivals

Battlefield departures now match only newly added graveyard object IDs with a
known card identity. An old identical card in the graveyard cannot make an exiled
creature appear to have died just because another card entered the graveyard.
Hidden identities are not matched to one another. Creature classification uses
the departing object's displayed face, rather than always the printed front.

A regression covers an exiled Stone Bear, an old graveyard copy, and an unrelated
new graveyard card. Existing genuine-death narration remains covered. Views do
not carry cross-zone object lineage, so simultaneous departures of identical
copies can still be ambiguous.

Code edits are confined to narrate.rs; engine/compiler work was preserved.

Validation: all 11 narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
