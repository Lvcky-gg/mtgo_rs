# Nonmana flashback costs — 2026-10-05

Flashback now accepts em-dash costs and supported nonmana payments: fixed life payments, filtered discards, sacrifices of matching battlefield permanents, and exiling matching graveyard cards. Variable quantities, tapping another permanent, and paying with the spell itself remain rejected. This enables cards such as Dread Return, Deep Analysis, and Lava Dart.

The engine now pays flashback life costs when casting from the graveyard, alongside ordinary additional casting costs. Normal hand casting does not pay the flashback cost.

Announcement validates one mana payment plan while reserving life and cards committed to other costs, then reuses that plan. Previously it replanned after paying additional costs and could proceed without spending mana when the second plan failed. Invalid announcements roll back rather than resolving unpaid. Automatic payment conservatively reserves selected cost permanents; a player can manually float mana before casting if a permanent needs to be tapped before being sacrificed.

Six new tests cover compiler acceptance/rejection, hand versus graveyard life payment and exile, insufficient life, sacrificing three creatures to reanimate another, combined flashback life and Phyrexian payments, and preventing one sacrificed mana source from paying two costs.

Coverage: 13,978 → 13,983 / 34,913 cards (40.1%). Universal support remains incomplete.

Validation: all-feature workspace suite passed (1,272 enabled tests, zero failures, 41 ignored). The last added double-sacrifice regression passed separately afterward. Final engine/oracle all-target/all-feature Clippy with warnings denied and whitespace checks passed. Workspace log: `/tmp/mtgo-flashback-workspace.log`. Concurrent edits preserved; changes uncommitted.
