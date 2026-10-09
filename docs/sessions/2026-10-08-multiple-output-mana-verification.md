# Whole-activation mana verification

Independent verification found two defects and a concrete-plan counterexample:
a larger GG ability hid a separate C ability, uniformly chosen repeated mana
was split across colors, and a feasible cost could receive activations incapable
of producing its recorded spend. Independent tests failed before the fix and
preserve the original shrunk seed.

Production now selects whole multi-output abilities and colors before invoking
the existing matcher. Ordinary one-mana sources keep their existing fast path.
Recorded activation choices remain consistent when the matcher spends a fixed
output before a flexible output from the same ability. A necessary capacity bound
rejects impossible costs early and excludes life-payable Phyrexian symbols.

Independent verification includes four direct oracle goldens, generated bounded
whole-activation enumeration, large impossible-capacity and Phyrexian life tests,
and two Engine casting goldens with permanent GameScenario fixtures. Expectations
for life, zones and cast availability were independently specified before
recording checkpoints. No VERIFIED promotion or global mutation-score claim.
Only a semantic-equivalent Clippy question-mark cleanup in player_shield accompanies
the mana change; concurrent compiler/card work is preserved.

Observed validation: all three mana property families passed with 10,000 cases
per family. Python79 passed/one skip. Workspace2089 passed/44ignored in 94
reported suites at that run's source snapshot. Workspace Clippy and formatting
passed after two routine fixes in concurrent card changes: equivalent `?` in
player_shield and an unused Target import/module formatting in Arcane Denial
compiled tests. No card behavior was changed by those cleanups.

Scoped two-mutant campaign killed both compiled defects with unchanged passing
baselines at Rust snapshot
`105955d2a9100d4440bf0e588d0ad1c3bc03d62787fc5cd65a33dee5f5b91df5`
(`/tmp/mtgo-multi-mana-mutants-final.json`). Later unrelated card-source edits
are outside that mutation snapshot. The first PR run executed all32 checks
successfully but correctly rejected evidence because concurrent sources changed;
the next stable campaign passed but became stale after the compiler-test cleanup.
Neither stale report is treated as current release evidence. Confidence remains
experimental and closed-alpha assessments remain unknown.

A third live campaign also executed all32 checks but rejected source drift.
An isolated snapshot at `/tmp/mtgo-multi-mana-source-snapshot` now holds exact
sources for reproducible validation. The snapshot caught concurrent entry-code
compilation using an undefined `face_down`; the guard now references the existing
`obj.face_down` in both live and snapshot trees. Its initial failed reports are
retained. Packaging inputs were also copied so the snapshot Python release tests
exercise their actual manifests. Snapshot evidence applies only to this frozen
source set, never to later edits of the live tree.

Completed frozen PR evidence: all32 planned checks passed, with eight replay and
fourteen regression fixtures and four semantic games. The snapshot-relative
normal gate passed; snapshot formatting, workspace Clippy and Python79/one skip
also passed. Its required embedded UI sound assets are preserved alongside source.
Independent reviewer confirmed the obj.face_down correction and passed existing
riot, morph, and external-enter-tapped tests in this frozen tree. Evidence report:
`/tmp/mtgo-multi-mana-snapshot-pr-final/report.json`. This reproducible snapshot
result does not imply that later concurrent live-tree edits have passed gates.

Frozen full workspace: 2090 passed, 44 ignored, zero failed across 94 reported suites. Log: `/tmp/mtgo-multi-mana-snapshot-workspace-complete.log`.
