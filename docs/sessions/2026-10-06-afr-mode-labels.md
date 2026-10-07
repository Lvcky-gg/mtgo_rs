# AFR named modal choices

Added sixteen explicit labels for eight AFR cards to strip_ability_word. Uses
existing effect-entry handling; no engine/IR changes. Unknown clauses still fail.

Frozen comparison: 16,125 → 16,132 / 34,913 (46.2%), seven gains, zero lost:
You Come to the Gnoll Camp, You Find the Villains' Lair, You Happen On a Glade,
You Hear Something on Watch, You Meet in a Tavern, You See a Guard Approach,
You See a Pair of Goblins. You Come to a River remains unsupported.
Snapshot /tmp/mtgo-adventure-modes-comparison; outputs
/tmp/mtgo-adventure-modes-before.txt and /tmp/mtgo-adventure-modes-after.txt.

Two parameterized gameplay tests verify Goblins' mass pump versus two token
creation, and Guard Approach's opposing creature tap versus friendly hexproof.
Oracle: 876 passed, 15 existing ignored. All-target Oracle Clippy passes.
