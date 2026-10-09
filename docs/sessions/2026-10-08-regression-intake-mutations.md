# Mutation verification for regression staging

Extended the disposable Python mutation campaign from five to ten curated
verification-tooling defects. The new mutations remove the review hash match,
direct behavioral assertion requirement, distinct reviewer check, observed-pass
requirement, and private snapshot integrity check from regression intake.
Independent intake tests reject each compiled mutant. No engine or corpus files
are modified by this campaign.

An independent reviewer additionally tested each additional test family's clean
baseline accounting: failed, empty, and errored controls cannot count as kills;
compile and import failures remain errors; previous mutated sources are restored
before the next family's baseline runs. All three independent accounting tests
pass. Full Python suite: 79 passed, one skipped. All ten selected mutations are
killed with passing unchanged controls. This is a curated harness sample, not a
repository-wide mutation score, independent Magic behavior evidence, or grounds
for confidence promotion. Weekly CI already invokes this runner and picks up the
expanded matrix.

Final current-source PR campaign: all 29 checks passed, preserving eight replay
and eleven regression fixtures plus four semantic games and their replay checks.
Normal release-evidence gate passed. Mutation evidence is archived at
`/tmp/mtgo-regression-intake-mutants-final2.json`; PR evidence is at
`/tmp/mtgo-regression-mutation-pr-final/report.json`.
