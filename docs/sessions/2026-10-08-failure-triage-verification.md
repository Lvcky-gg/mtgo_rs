# Bounded failure triage

Failed corpus replays, semantic campaigns and generated replay checks now invoke
best-effort offline triage before the campaign archives its failed report. Diagnostic
commands remain outside the planned passing check inventory. Triage cannot erase the
original failure, and the gate rejects nonempty failure-triage evidence even if a
report incorrectly claims all checks passed.

The five stages capture the original replay report, independently reproduce it,
minimize its scenario, capture the candidate report and independently reproduce
that candidate. Each command is capped at 30 seconds. Failure identity compares
the normalized message, complete failing action content and canonical diff paths;
the action index may shift. Assertions/provenance and non-object setup must remain
unchanged. Diagnostics retain errors, incomplete reductions and original evidence.
No fixture or confidence state is automatically promoted.

Independent review found a preservation defect during implementation: a faulty
reducer could overwrite the caller's original before the final comparison detected
the change. Commands now receive an exclusively created private snapshot, checked
with bounded reads after every stage. The caller's source stays untouched. Tests
also verify that successful or crashing diagnostics do not replace the original
campaign failure and cannot satisfy a release gate.

Validation: 72 Python tests pass with one intentional skip; formatting and diff
checks pass. Two real Engine integrations reproduce advance-budget exhaustion and
the checkpoint-substitution fixture through all five stages, preserving their
failures and external inputs. These are deliberate harness failures, not newly
discovered Magic-engine defects. The fresh PR campaign passes 29 checks with 8
replays, 11 regressions and 800 observed semantic actions; its current-source gate
passes. The closed-alpha gate still correctly fails its three unknown assessments.
Hosted CI failure triage has not been executed; orchestration failure tests use
explicitly synthetic mocked commands.

Five selected compiled Python gate/intake mutations are killed, including the two
new mutations restoring failure substitution and exposing the external original
to the reducer. Each test family has its own passing nonempty control, sources are
restored between mutants, and syntax compilation/test runs avoid bytecode writes.
Errors and empty campaigns never count as kills. Final campaign and mutation
evidence share source fingerprint
`f5255decdc4b9021bc02baba6954af3f3f9123abb4e44749afe9fbc42fcdc6b9`.
Rust engine code, production Rust mutation matrix and full workspace tests were
unchanged/not rerun this turn; the PR includes the full verification-crate suite.

Triage returns a bounded candidate, not proof of globally minimal or causally
equivalent Magic behavior. Property failures still retain their own logs and shrunk
seeds; panic/nontermination without a serialized scenario cannot be reduced through
this path. Automatic rules classification, regression intake and PR generation
remain future work. No confidence promotion or release-readiness claim.
