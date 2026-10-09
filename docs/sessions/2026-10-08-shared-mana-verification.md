# Shared-permanent mana verification

Independent generated enumeration found a confirmed planner defect after the
recent shared tapping-source fix: merging G and C abilities preserved the colored
choice but dropped the colorless choice. An explicit `Unit.colorless` capability
now preserves that alternative in grouped sources and ordinary pool/source units.
Generic-only helper restrictions retain their previous early rejection.

The independent verifier authored three planner goldens, one Engine casting
and payment golden, and a 10k-case grouped exhaustive property. It checks one
activation per permanent, matching ability/output identity, payment capacities,
colored/colorless obligations, and exact generic payment. Original shrinking
seed is permanent. The Engine scenario specifies life23 and final zones before
recording, confirms cast availability and one C mana produced/spent, and is
stored as `tests/regressions/issue_local_mana_colorless_alternative.json`.

Scoped three-mutant production campaign tests double-spending one permanent,
recording the wrong ability, and dropping the colorless alternative. This is
curated empirical evidence, not a global mutation score or VERIFIED promotion.
Scope excludes hybrid/Phyrexian, restrictions, costly/repeatable activation,
sacrificing abilities, and multi-mana outputs. Other production mechanics are
unchanged; the separate ongoing compiler/card work is preserved.

Final validation: workspace 2,077 passed, 44 ignored, zero failed across 92
reported suites; all five independent tests include the engine-level golden.
Clippy for engine/verification and formatting passed. Python 79 passed/one skip.
PR campaign now has 30 passing checks: eight replays, twelve regressions, four
semantic games and their replay checks. Current-source gate passed; all three
closed-alpha issue assessments remain unknown. Three selected compiled mutants
killed with passing unchanged baselines at Rust snapshot
`abd7eaa3b981c5d1128827527deaba377e2fbc088c6d7ab846d00fc3e0f758fe`.
Evidence: `/tmp/mtgo-shared-mana-workspace.log`,
`/tmp/mtgo-shared-mana-mutants-final.json`, and
`/tmp/mtgo-shared-mana-pr-final/report.json`.
