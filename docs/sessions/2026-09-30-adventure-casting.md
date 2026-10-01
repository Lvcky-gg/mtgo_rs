---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, rules]
---

# Adventure casting and casting the normal card from exile

Adventurer cards now offer their normal Cast action and a CastFace(face=1)
Adventure action. Each evaluates its own characteristics, timing, cost and targets,
using the face-selection machinery introduced for MDFCs. Outside the stack, an
adventurer uses its normal face.

A successfully resolving Adventure moves to exile with its normal characteristics.
AdventureExiled grants its resolving controller permission to play this exact exile
object. GameObject.adventure_player and a public serde-default ObjectView field
record the player. Exile permission grants only the normal play, with ordinary
cost and timing, and resets on any zone change. Ownership is independent from the
player who receives permission. Arbitrary exile, countering and failure to resolve
due to illegal targets grant no permission.

GUI priority labels identify casting from adventure. Exile/graveyard browsers now
let players click only engine-offered plays, with a choice when more than one is
available. Normal/adventure alternatives already have their own text snapshots,
labels, printed costs and preview data. Bot snapshots both faces and prefers an
available Adventure over the same card's normal spell so it can use both halves.
It subsequently casts the normal card from exile when offered.

The strict gate admits exactly two adventure faces with a normal permanent face
and an instant/sorcery Adventure, only when all printed text compiles. Shared GUI
and CLI coverage counts 8,551 / 34,898 (24.5%), +52 over MDFC support. Measured on
the isolated temporary database used to validate the Scryfall import fix; no user
collection modified.

Validation:
- cargo test --workspace --all-features: 706 passed, including socket matches.
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean.
- Eight new compiler-to-engine Adventure scenarios cover successful resolution,
  casting normally and from exile, counters, illegal targets, arbitrary exile,
  timing, permission lifetime and player attribution, and bot use of both halves.
- Import gate regression rejects unsupported Adventure text, wrong alternative
  types and missing faces.
- GUI rebuilt with cargo build -p mtg-app --bin mtg-gui. Interactive GUI not run.

Checked official Comprehensive Rules section 715 (June 19 2026):
https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf

Remaining: split casting, transforming cards, prevention, copy effects, and the
known shuffle-seed information leak. Copies and special external exile-casting
permissions are not introduced by this change; the existing strict compiler gate
continues to reject unsupported effects.

Brain remains outside writable roots; handoff saved in repository. No commit
created; sources remain untracked.
