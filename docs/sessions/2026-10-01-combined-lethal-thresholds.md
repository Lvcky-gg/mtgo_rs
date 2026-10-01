# Preserve combined lethal thresholds

Forced combat-damage detection now sums individual blocker lethal thresholds in
u64 and compares widened attacker power. Multiple large but individually valid
thresholds previously overflowed u32, causing a debug panic or incorrect prompt
decisions in release.

A regression uses three blockers with i32::MAX toughness against a 6-power
attacker. It reproduced an addition overflow before the fix and verifies the
assignment remains forced after it.

Validation: all 29 combat tests, engine library/combat-test Clippy with dependency
lints disabled and warnings denied, and diff checks pass. Source formatted.
Changes remain uncommitted.
