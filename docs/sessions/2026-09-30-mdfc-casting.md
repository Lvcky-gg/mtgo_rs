---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, rules]
---

# Modal double-faced casting and land play

Completed face-aware MDFC play on top of the layout preservation work. Existing
Cast and PlayLand actions remain front-face actions. New CastFace and PlayLandFace
variants explicitly select the back. The engine independently evaluates each
face's timing, mana, additional costs, targets and graveyard permission. Land play
is restricted to hand, own main phase, empty stack and available land play; a land
back cannot be played from the command zone or via a front's flashback.

FaceSelected events carry the selection into stack/battlefield state. Resolving
permanent spells retain their face; other zone transitions reset to front. Undo
restores the original front-face hand object and payment. Moving an MDFC directly
to the battlefield uses the front, and instant/sorcery fronts stay in their zone.

Triggered abilities snapshot their face, including trigger footprints, ordering,
labels and placement. Dies/leaves triggers use the departed battlefield face
instead of the graveyard front, with existing new-identity self-return semantics.

ObjectView projects visible face with a backward-compatible default; hidden
objects expose zero. UI text snapshots retain both faces, board land rows and art
lookup use the selected face, action labels name that face and its printed cost,
and clicking a card with multiple legal plays opens a face choice in the question
panel with per-option hover details and cancellation. Copy deduplication preserves
face choices. Bot stats, loyalty and targeting metadata are keyed by face; the bot
can select land backs or expensive spell backs and target their own effects.

Strict GUI/CLI playability uses a shared layout gate: exactly one normal face or
two modal_dfc faces, with all rules text compiled. Other layouts remain excluded.
Measured coverage on a read-only SQLite backup: 8,499 / 34,898 (24.4%), +27 cards.

Validation: cargo test --workspace --all-features: 693 passed, including transport
socket suites. cargo clippy --workspace --all-targets --all-features -- -D warnings:
clean. Fifteen new compiler-to-engine MDFC scenarios and three UI/import regressions
cover costs, effects, target/timing independence, tapped land entry/mana, both
spell/land orientations, death triggers, bounce, invalid faces, flashback,
Commander land restrictions, undo, bot decisions, wire round trips and strict gate.
GUI compiled and pure presentation logic tested; no interactive visual exercise.

Rules checked against official June 19 2026 Comprehensive Rules, section 712:
https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf

Split/adventure/transform casting, transformation effects, copy effects,
prevention and the documented shuffle-seed information leak remain outstanding.
Last user steering: something is wrong with downloading from Scryfall; investigate
bulk data fetch and image fetch paths next. Brain remains outside writable roots.
No commit created; sources remain untracked.
