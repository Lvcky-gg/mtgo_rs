---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, rules]
---

# Plain split-card casting

Split cards now offer either half as a separate legal cast. Each half uses its own
cost, timing, targets, and spell effects. The engine evaluates the selected half
during casting and on the stack, then restores combined characteristics after it
leaves the stack. Graveyard permissions, including half-specific flashback, use
the selected half's abilities and costs.

PrintedCards exposes combined split characteristics: both names, mana costs,
colors, types, and abilities. Engine evaluation and layers agree on these outside
the stack. The strict gate excludes abilities that need unsupported execution
outside the stack; it admits supported spell effects, keywords, casting costs and
permissions, self statics, and stack triggers only.

The GUI displays both halves outside the stack and the selected half on the stack.
Its existing card-choice panel offers each legal half with separate previews.
Narration uses the same object text. Deck costs and curves use combined mana value.
The bot considers each offered half's cost and effects while valuing the complete
card outside the stack.

The shared import and CLI coverage gate accepts exactly two instant/sorcery split
halves whose complete text and ability shapes are supported. Fuse, aftermath,
split permanents such as Rooms, and unsupported off-stack activated or triggered
abilities remain excluded. No fused cast action or Room unlocking was added.

Validation:
- cargo test --workspace --all-features: 715 tests passed, including socket tests.
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean.
- Regression tests cover combined characteristics, selected-half costs, timing,
  targets, resolution, flashback, bot choices, object display, deck curves, and
  rejection of unsupported split shapes.
- Coverage: 8,567 / 34,898 (24.5%), +16 over Adventure support, measured using
  /tmp/mtgo-scryfall-fixed.sqlite without modifying the user collection.
- cargo build -p mtg-app --bin mtg-gui succeeded. Interactive GUI not exercised.

Checked official Comprehensive Rules section 709 (June 19 2026):
https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf

Remaining: fuse/aftermath and split permanents, transforming cards, prevention,
copy effects, and the known shuffle-seed information leak.

Brain remains outside writable roots; handoff saved in repository. No commit
created; sources remain untracked.
