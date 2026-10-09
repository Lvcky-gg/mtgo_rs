# Alternative mana-cost verification

Independent mana-symbol enumeration found two confirmed defects: `{1}{G/P}` was
rejected with G1 and sufficient life, and nine `{2/W}` symbols were rejected with
C18 because the production mask covered only eight symbols. The original shrunk
property seed is preserved. Production frees optional Phyrexian mana assignments
for mandatory generic symbols and pays those Phyrexian symbols with life. Full
monohybrid allocation now uses prefix feasibility and memoized failures without
cost truncation or machine-sized shifts. Existing matching remains in place.

Independent tests verify feasibility and actual spend/life assignments. Direct
boundaries include 65 monohybrid symbols, insufficient generic payment and exact
life payment. A permanent Engine regression specifies life21 and final zones
before recording and additionally checks cast availability, life18 after paying,
source tapped, zero pool, G1 production/spend and LifeChanged(-2).

Four mana property families passed with 10,000 cases each in the live-tree run.
The new alternative-cost suite has eight tests, independently authored and
reviewed. Python79 passed/one skip. Concurrent card edits continue, so reproducible
whole-workspace evidence uses the previous passing frozen baseline plus only
this increment's owned mana/test/fixture/mutator files, located at
`/tmp/mtgo-alternative-mana-source-snapshot`. It does not claim coverage of later
live card edits. Confidence remains experimental.

Final frozen validation: 2098 workspace tests passed, 44 ignored, zero failed across 95 suites. All33 PR checks passed (eight replays/fifteen regressions/four semantic games), normal gate PASS, formatting/workspace Clippy PASS, Python79/one skip. Both selected compiled mutants were killed with unchanged passing baselines; their Rust source fingerprint matches this frozen source set. Reports: `/tmp/mtgo-alternative-mana-snapshot-pr/report.json`, `/tmp/mtgo-alternative-mana-snapshot-mutants.json`, `/tmp/mtgo-alternative-mana-workspace.log`. No confidence promotion or claim for later live changes.
