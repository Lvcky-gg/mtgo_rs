# Two-quality library selection — 2026-10-06

Optional “a creature card and/or a land card” selections now use one quota for
each quality, rather than flattening the filters into a shared count. Effect::Dig
has a serde-defaulted additional_filter for a second optional one-card slot from
the same top group and destination. IR walking visits the additional filter.

The resolver chooses both slots before moving either card. Previously selected
cards are excluded from the second slot, including cards matching both qualities.
Invalid duplicate answers are ignored by the existing choice sanitizer. When the
chosen cards must be revealed, all reveals happen before any selected card moves.

The compiler accepts optional singular “and/or” quotas with the same destination.
Different destinations, mandatory combinations, and total-mana-value limits remain
rejected. Existing single-quota library selections keep their behavior.

Frozen-source comparison: 15,664 → 15,668 / 34,913 (44.9%), +4, zero losses.
Added Benefaction of Rhonas, Gift of the Gargantuan, In the Presence of Ages,
Relentless Pursuit. Claude's concurrent changes were held constant.

Four gameplay regressions exercise all skip/take combinations, overlapping
creature/enchantment qualities, duplicate answers, empty matching groups, private
look versus public reveal, remainder destinations, and reveal/move ordering.
The previous negative test now rejects a mandatory two-quality combination.

Validation: 1073 tests passed, 38 existing ignored across mtg-ir,
mtg-engine, mtg-oracle. Clippy with warnings denied, GUI build, and diff check pass.

Changes confined to Dig's IR field/resolver/walk, clauses::dig, new
compiled/two_quality_dig.rs and test registration, existing negative regression,
README and coordination documentation.
