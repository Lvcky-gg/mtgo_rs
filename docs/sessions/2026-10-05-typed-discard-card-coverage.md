# Typed discard costs — 2026-10-05

Extended the compiler's chosen-cost grammar to accept typed discard costs, including creature, land, artifact, color, subtype, nonland, and instant-or-sorcery card filters. Both activated abilities and mandatory additional casting costs use this grammar. Existing engine cost selection/payment implements the behavior; no engine implementation changes were needed.

The grammar requires card/card(s) with matching quantity and supplies the hand zone explicitly to the noun parser. It continues rejecting random discard, alternative payments, and malformed quantities rather than claiming unsupported behavior works.

Coverage against a read-only backup of the installed database: 13,947 before this batch, 13,971 afterward, of 34,913 cards (40.0%). Concurrent card-support work is preserved; these counts describe the current database/compiler coverage, not comprehensive rules conformance for every accepted card. Universal card support remains unfinished.

Three regression tests: one compiler test covering accepted filters and rejected costs, plus two compiled-game tests exercising affordability and actual payment for an activated ability and a spell. Cards in opponents' hands or on the battlefield cannot pay a discard cost.

Validation: oracle all-feature tests passed (77 unit, 448 compiled-game, 35 import, 10 import-atomicity; 15 ignored). Final workspace validation recorded below. Changes are uncommitted.

Remaining high-frequency unsupported shapes include dredge, conspire, learn, assist, banding, cipher, splice, enlist, firebending, blitz, encore, and Fuse. Several require new engine choices/state rather than parser-only recognition. `/tmp/mtgo-coverage-before.txt` contains ranked unsupported lines from this audit.

Final checks: all-feature workspace tests passed; oracle all-target/all-feature Clippy with warnings denied and whitespace checks passed. Test log: `/tmp/mtgo-playability-workspace.log`.
