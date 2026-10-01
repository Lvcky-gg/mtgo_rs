---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, rules]
---

# Global combat-damage prevention

The compiler now accepts exactly “Prevent all combat damage that would be dealt
this turn” as Effect::PreventAllCombatDamage, including composition with supported
additional clauses. Other prevention scopes and numeric shields remain rejected.

Resolution emits CombatDamagePreventionChanged(active=true), applied to a public
GameState flag. Combat damage emits no damage events while active, suppressing
lifelink, poison, commander damage, deathtouch, loyalty loss, and damage triggers.
Both first-strike and regular steps are covered. Noncombat damage is unaffected.
Cleanup emits the corresponding inactive event alongside end-of-turn expiration.
Multiple global effects share the flag because they all expire at the same cleanup.
The effect footprint is conservative so trigger ordering is not assumed independent.

PlayerView projects the public flag with a serde default for older snapshots.
The GUI phase bar displays active prevention; narration announces start and expiry.
The bot snapshots pure prevention spells and holds them until an opposing declared
attack threatens its player or permanent, then casts an engine-offered spell after
blockers or first-strike damage. It avoids redundant casting while prevention is
active. This is a simple heuristic, not a combat survival calculation; composite
prevention spells retain ordinary spell heuristics.

Coverage: 8,592 / 34,898 (24.6%), +17 over aftermath support, measured against
/tmp/mtgo-scryfall-fixed.sqlite without modifying the user collection.

Validation:
- cargo test --workspace --all-features: 728 tests passed, including socket tests.
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean.
- Five compiler-to-engine scenarios cover double strike, lifelink, draw triggers,
  blocking, infect, deathtouch, planeswalker loyalty, noncombat damage, cleanup
  expiration, public projection, and countering the prevention spell.
- Compiler regression verifies composition and rejects unsupported scopes/shields.
- Client narration and bot defensive-use regressions pass.
- cargo build -p mtg-app --bin mtg-gui succeeded. Interactive GUI not exercised.

Checked CR 615 and 514.2 in the June 19 2026 Comprehensive Rules:
https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf

Fuse was inspected but not implemented: it needs combined casting characteristics,
target slots, costs and left-then-right resolution. Remaining work also includes
targeted/numeric prevention, split permanents, transforming cards, copy effects,
and the known shuffle-seed information leak. Unpreventable damage and prevention
triggers remain unsupported and excluded by the strict compiler.

Brain is outside writable roots; handoff saved in repository. No commit created;
sources remain untracked.
