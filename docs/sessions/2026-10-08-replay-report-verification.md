# Replay report ingestion and boundary verification

`scenario report INPUT.json REPORT.json` exports a versioned attachment containing
the unchanged GameScenario and observed RunReport. Failed scenarios still export
their evidence and return exit1. `report replay REPORT.json` independently executes
the nested scenario and compares the entire outcome: message, first divergent
action, digest, states, diffs and pass status. Exact reproduction returns exit0
with `reproduced: true`, even when the observed scenario fails. Captured observations
never replace assertions or promote a regression.

Report parsing rejects unknown fields/versions and bad nested scenarios. It bounds
outer input at 64MiB and preserves the scenario 16MiB limit. Additive `create_new`
writes protect existing attachments. Structural fuzz parsing includes this boundary.
Artifacts contain private cards/state; export is offline from existing scenarios,
not yet wired into live client recording or public issue submission.

Independent review exposed two reproducible defects. A zero-action semantic
campaign captured raw initial state while replay first advanced to a choice or game
end, so its ostensibly successful artifact failed replay. Campaigns now normalize
setup through that same boundary before recording any actions. Initial and terminal
invariants now run even if no further choice exists. That additional check revealed
a legal concession leaving priority with the departed player after game over.
The production terminal transition now clears priority. A one-answer/no-object
permanent regression and an independent terminal golden check the remaining winner,
departed loser and absence of priority (CR104.1/104.3a).

Eleven independently authored report/boundary tests cover both defects, zero-action
terminal state, invalid initial counters, passing/failing report round trips,
metadata/assertion preservation, tamper rejection, parser limits and CLI status/
overwrite behavior. Validation passes: 2,032 workspace tests, 44 ignored; fmt;
workspace/all-target Clippy with warnings denied; diff checks; Python 18 tests with
one skipped. PR campaign passes 29 checks: 8 replays, 11 regressions and 800 observed
semantic actions. An early parallel crate run lost a build-wrapper executable during
concurrent rebuilding; the completed workspace and PR runs pass and include all
eleven new checks. The infrastructure failure is not counted as an engine finding.

Three selected compiled production mutants are killed in two isolated campaigns:
terminal priority retention and trusting captured observations (2/2 at Rust snapshot
`32ee29b273770a111571d56f0f4d9dea55e67c35165462da589489dcc07836f2`),
and skipping zero-action normalization (1/1 at
`33a885cd5b676dd262227b3c7559c53f67f5d12b4bd8c53033b37310ba45e4ff`).
There are 26 curated mutations configured; this turn did not rerun the other 23.
These snapshot-scoped results are not a repository mutation score.

Parse/setup errors, panics and advance-budget errors still return errors rather
than structured attachments. Rules classification, causal minimization review and
regression intake remain independent steps. No confidence promotion or release
readiness claim.
