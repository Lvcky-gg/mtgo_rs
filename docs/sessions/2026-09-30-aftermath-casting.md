---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, rules]
---

# Aftermath split-card casting

Aftermath now compiles to a dedicated AbilityKind::Aftermath marker. The engine
offers that half from its owner's graveyard for its printed cost, using ordinary
timing and target requirements, and forbids casting it from other zones. The
normal half remains independently playable from hand and goes to the graveyard
normally. The existing graveyard browser and bot consume these legal actions.

Casting uses the existing exile_on_leave stack context. Resolution, counters, and
failure to resolve because all targets are illegal exile the complete card with
combined characteristics. No Adventure permission is granted. Also fixed the
generic resolution zone-movement path: flashback and aftermath now replace moves
from the stack to hand or library with exile, clearing the requested zone index.
Previously that replacement was handled only for resolution and countering.

The shared card gate now accepts fully compiled aftermath split cards. Fuse, split
permanents, unsupported off-stack split abilities, and transforming layouts remain
excluded. Coverage is 8,575 / 34,898 (24.6%), +8 over plain split support. Measured
against /tmp/mtgo-scryfall-fixed.sqlite; the user collection was not modified.

Validation:
- cargo test --workspace --all-features: 720 passed, including socket tests.
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean.
- Four additional compiler-to-engine scenarios cover graveyard-only casting,
  printed cost, sorcery timing, successful resolution, counters, illegal targets,
  and the normal half's graveyard destination.
- Import regression accepts aftermath with reminder text while rejecting fuse,
  split permanents, and unsupported off-stack abilities.
- Engine regression checks replacement of hand/library/graveyard destinations,
  the emitted event, and unchanged destinations without exile_on_leave.
- cargo build -p mtg-app --bin mtg-gui succeeded. Interactive GUI not exercised.

Rules verified against CR 702.127 in the June 19 2026 Comprehensive Rules:
https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf

Remaining: fuse and split permanents, transforming cards, damage prevention,
copy effects, and the known shuffle-seed information leak.

Brain is outside writable roots; handoff saved in repository. No commit created;
sources remain untracked.
