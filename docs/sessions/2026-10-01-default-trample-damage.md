# Preserve trample damage when accepting the default

Accepting an AssignCombatDamage default now uses the complete canonical combat
assignment, including a defending-player target. The wire answer stores only
object rows, so the previous handler applied blocker damage but silently dropped
excess trample damage to the player. Custom rows retain their existing handling;
custom assignment validation remains follow-up work.

The new end-to-end combat regression sends a 4/4 trampler into two 1/1 blockers,
accepts the prompted default, and checks both blockers die and the defender loses
2 life. It failed before the fix (20 life instead of 18) and passes after it.

Validation: all 25 combat integration tests, engine library Clippy with dependency
lints disabled and warnings denied, and diff checks pass. Changed source formatted.
The patch is confined to the damage-answer branch; concurrent engine edits were
preserved. Changes remain uncommitted.
