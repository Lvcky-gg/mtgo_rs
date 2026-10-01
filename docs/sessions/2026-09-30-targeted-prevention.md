---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, rules]
---

# All-damage prevention for targeted recipients

The compiler accepts “Prevent all damage that would be dealt to <target recipient>
this turn” as Effect::PreventDamage. Creature, player, and any-target noun phrases
use the normal target constraints. Only targeted selectors are admitted; numeric
shields, source-specific prevention, other durations, and untargeted groups remain
rejected rather than approximated.

Resolution records DamagePreventionChanged in the event log. A unique recipient
list in GameState prevents damage in both spell resolution and combat damage,
before lifelink, poison, loyalty loss, deathtouch, and damage triggers are applied.
Cleanup emits expiry events. Zone changes discard prevention of the old object
identity, so a bounced and recast card is not protected. Illegal targets at
resolution produce no prevention.

PlayerView exposes the public recipient list with a serde default. The phase bar
names protected recipients, and narration announces prevention and expiry using
only visible names. Bot target preferences treat prevention as beneficial.

A regression also exposed and fixed an existing recipient-selection bug:
targets_of combined object evaluation with implicit controller evaluation, so
“damage to each creature” also damaged each controller. Object-only selectors now
stay object-only, while explicit player selectors and unions retain players.
Union/except recipient sets recurse without introducing implicit controllers.

Validation:
- cargo test --workspace --all-features: 737 tests passed, including socket tests.
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean.
- Six new prevention scenarios cover spell damage to objects/players, friendly
  combatant protection, poison and expiry, bouncing/recasting, illegal targets,
  and mass damage/lifelink with protected and unprotected recipients.
- An additional mass-damage regression distinguishes each creature from each
  creature and each player.
- Compiler and client narration regressions pass.
- GUI rebuilt with cargo build -p mtg-app --bin mtg-gui; interactive GUI not run.

Coverage: 8,598 / 34,898 (24.6%), +6 over global combat prevention. Measured on
/tmp/mtgo-scryfall-fixed.sqlite; no user collection was modified.

Remaining: numeric and source-specific prevention, unpreventable damage,
prevention triggers, fuse, split permanents, transforming cards, copy effects,
and the known shuffle-seed information leak.

Brain remains outside writable roots; handoff saved in repository. No commit
created; sources remain untracked.
