# Inline library count definitions — 2026-10-06

The compiler now accepts inline definitions for top-X library selections, such as
“where X is the number of artifacts you control”, followed by selection instructions.
The effect parser routes those definitions to Dig. Dig stores the existing parsed
Value expression as its count, evaluated when the selection resolves.

This is limited to the top-card count. It does not enable later references to X
or broaden general X substitution. Unknown definitions, undefined X, and conflicts
with a casting X remain rejected. No engine or IR changes were needed.

Frozen-source comparison: 15,719 → 15,725 / 34,913 (45.0%), +6, zero losses.
Added Fomori Vault, Machinate, Muzzio, Visionary Architect, Nessian Game Warden,
Seismic Sense, Stirring Honormancer. Claude's concurrent changes were held constant.

Four regressions verify artifact-dependent counts, zero count, an entering
creature counting in its own triggered selection, and unsupported definitions.
They pass in the shared workspace and frozen snapshot. Oracle suite: 814 passed,
15 existing ignored. Clippy with warnings denied and diff check pass.

Clippy also required a mechanical `ability: ability` → `ability` cleanup in
Claude's new compiled/mana_followups.rs test. No behavior changed there.

Files: clauses::effect dispatch guard and clauses::dig count parsing,
compiled/defined_dig.rs plus registration, the one test initializer cleanup,
README and coordination documentation.
