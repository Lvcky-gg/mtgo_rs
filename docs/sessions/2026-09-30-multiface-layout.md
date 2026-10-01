---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, rules]
---

# Preserve multi-face layout through card loading and match snapshots

First prerequisite for modal double-faced casting. The stored database already
retains the layout and all faces, but CardLibrary and GameCards discarded the
layout. The engine could not distinguish modal, transforming, split, or adventure
cards by its PrintedCards interface.

PrintedCards now exposes layout; existing single-face fixtures default to Normal.
Layout is exported from mtg-ir and includes Unknown for unfamiliar imported
layouts. Conversion maps the existing supported vocabulary without interpreting
unknown strings as normal cards. CardLibrary retains each imported layout,
including sparse-id Unknown values and generated Token entries.

SourcedCard carries layout explicitly, LocalSource converts it from stored rows,
and demo sources specify Normal. GameCards preserves layout while renumbering
cards and serializes it beside faces. Older serialized tables lacking layouts
still deserialize, with Unknown relationships rather than guessed face rules.
The stale conversion documentation was corrected to describe the rules compiler.

Validation: cargo test --workspace --all-features: 675 passed (including local
socket matches). cargo clippy --workspace --all-targets --all-features -- -D
warnings: clean. Three new tests check imported split relationships, all layout
conversions including unfamiliar values, and a two-face modal match serialization
round trip plus legacy snapshots.

MDFC casting is NOT implemented yet. Playability and coverage gates remain
unchanged, and back faces are not offered as actions. Next work:
- Add explicit face-aware Cast/PlayLand choices while keeping existing actions.
- Evaluate selected-face timing, mana, additional costs and targets.
- Preserve selected face from stack to battlefield; reset to front in other zones.
- Project face through PlayerView and render correct text/art/actions.
- Update bot metadata and decisions for both faces.
- Integrate invented spell/land and spell/spell MDFC scenarios, invalid face
  rejection, rollback, zone transitions, and wire compatibility before enabling
  MDFCs in the strict compiler and CLI coverage gates.

Brain remains outside writable roots, so this handoff is in the repository.
Sources remain untracked; no commit created.
