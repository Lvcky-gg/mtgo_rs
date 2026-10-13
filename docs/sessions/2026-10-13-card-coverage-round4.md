# Card coverage, round 4 — 2026-10-13

Continued the existing uncommitted card coverage work. Against the same database
snapshot, coverage rises from 17,567 to 17,592 / 34,914 (50.4%): 25 additions and
zero losses in the complete card-name comparison. All four rounds together add
168 playable cards over the fresh HEAD baseline of 17,424. The name-check command
continues to report five more playable cards than coverage, a pre-existing lookup
difference (17,597 versus 17,592).

## Library and graveyard searches

Added a general one-card search shape for "search your library and/or graveyard"
with a printed quality, optional reveal, and hand or battlefield destination. The
player explicitly chooses library, graveyard, or both. Searching the library causes
a shuffle even when no card is found or the chosen card comes from the graveyard.
Searching only the graveyard does not shuffle and must find an available matching
card in that public zone. Existing legacy optional fixed-name search handling is
unchanged.

Newly playable: Captain Kirk, Boldly Going; Chandra's Outburst; Delivery Moogle;
Elspeth, Undaunted Hero; Finale of Devastation; Sun-Blessed Mount; Tower Winder.

Four compiled-card tests cover all three zone choices, failed library searches,
Delivery Moogle's reveal and mana-value restriction, and Finale's X=10 pump/haste
for both the found creature and existing creatures. The bonus expires correctly
at the end of the turn. The public-zone choice test also checks the complete
candidate list and mandatory one-card selection.

## Enchanted and equipped filters

Added a subtype-specific attachment filter and adjective parsing for enchanted and
equipped objects. Plural "enchanted creatures" and "equipped creatures" now reach
the general noun parser instead of being prematurely consumed as the singular
source-relative "enchanted creature" / "equipped creature".

For death triggers, departing permanents retain the copiable identities of their
attachments in last-known information. Simultaneous batches capture these before
any removal, so an Equipment destroyed earlier in the same batch still counts.
An Equipment destroyed by an earlier effect does not count when its former bearer
later dies. The new zone identity clears the attachment snapshot. Empty snapshot
metadata is omitted from verification serialization; no fixtures needed updating.

Newly playable: A Tale for the Ages; Barret Wallace; Concord with the Kami;
Cut the Earthly Bond; Enchanted Being; Feast of Dreams; Greater Auramancy;
Item Shopkeep; Kestia, the Cultivator; Kor Blademaster; Nyx Herald; Ramses Overdark;
Resistance Reunited; Stone Haven Outfitter; Temple Thief; Venomous Vines;
Wall of Putrid Flesh; Zamriel, Seraph of Steel.

Four compiled-card tests check Aura versus Equipment distinctions, controller
restrictions, removal of an Aura's anthem bonus, Greater Auramancy's shroud,
Stone Haven Outfitter's buff and death draw, simultaneous attachment/bearer deaths,
and removal in separate effects without a phantom death trigger.

Validation: `cargo test --workspace --quiet`, including the complete replay and
regression corpus; targeted tests for the final search-choice assertions;
`cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all --check`;
and `git diff --check`. Workspace tests ran outside the sandbox for their existing
loopback networking tests. All work remains uncommitted.
