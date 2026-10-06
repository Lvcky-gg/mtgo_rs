# Empty battlefield conditions and trigger resolution — 2026-10-05

The compiler understands “if no creatures are on the battlefield” and “if there are no creatures on the battlefield,” plus supported plural battlefield nouns such as artifacts. It counts matching objects globally unless the noun supplies a narrower restriction. This enables Pestilence/Pyrohemia shapes.

A regression exposed a missing engine check: intervening-if clauses were evaluated when triggers went on the stack but were not checked again at resolution. Trigger resolution now evaluates the condition before executing the effect; if false, the effect does nothing and the ability finishes normally. Creating a creature in response therefore preserves a Pestilence-like enchantment. The fix also applies to other printed and granted triggered abilities using intervening-if conditions.

Three new tests cover complete grammar and rejected variants, creatures controlled by either player versus an empty battlefield, and a creature entering in response before the trigger resolves. The response test failed before the engine fix and passed afterward.

Coverage: 14,004 → 14,008 / 34,913 cards (40.1%). Universal support remains incomplete. Concurrent edits preserved; changes remain uncommitted.

Validation: targeted regressions and engine/oracle all-target/all-feature Clippy with warnings denied passed. Full workspace result recorded below. Log: `/tmp/mtgo-empty-battlefield-workspace.log`.

Final all-feature workspace result: 1287 passed, zero failures, 41 ignored. Whitespace checks passed.
