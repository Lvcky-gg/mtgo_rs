# Self or another creature combat damage triggers

Union noun event table accepts deals combat damage to a player, using existing
DealsDamage with combat_only true and EventSubject binding. No engine/IR changes.
Parameterized union_combat.rs test covers source, matching friendly subtype,
wrong subtype, damage to a blocking creature and noncombat damage to a player.

Frozen comparison: 15,999 → 15,999 / 34,913 (45.8%), zero gains or losses.
Tyranid Harridan's plain triggered clause now parses, but its printed named ability
word remains a separate blocker. No full-card coverage gain claimed.
Snapshot /tmp/mtgo-union-combat-comparison; outputs /tmp/mtgo-union-combat-before.txt
and /tmp/mtgo-union-combat-after.txt.
Oracle suite: 859 passed, 15 existing ignored. All-target Oracle Clippy passed.
Claude's previously failing died-this-turn regression now passes in the live suite.
