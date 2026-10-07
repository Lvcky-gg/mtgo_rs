# Builder and verifier handoff

Assign implementation and verification separately. Give the Rules Lawyer and
Breaker the requested observable behavior, supported scope, relevant CR sections
and existing public engine interfaces before sharing the Builder's explanation.
Independent agents may read production code to reproduce a failure; their
expected behavior must not derive solely from its current output.

| Role | Deliverable |
| --- | --- |
| Builder | Minimal production change and supported/unsupported scope |
| Rules Lawyer | CR proposition, independent assertions and golden test |
| Breaker | Counterexample or recorded attempted attacks |
| Property Agent | Generalized invariant and generated state/action families |
| Fuzz Agent | Bounded structural/semantic campaign and retained seed artifacts |
| Differential Agent | Scoped independent model, provenance and disagreement |
| Minimizer | Smaller reproduction retaining the same failure class |
| Reviewer | Assessment of implementation and independent evidence |
| Release Judge | Observed campaign/gate report, unresolved issues and uncertainty |

The Builder's own tests are useful but cannot alone establish VERIFIED status.
Review names in evidence files are advisory; verify their underlying reports and
build identities before accepting a confidence promotion.

For a discovered failure:

1. Preserve the original artifact and campaign log before edits.
2. Reproduce with `mtgo-rs replay FAILURE.json`. Distinguish a rules defect from
   a generator, assertion or serialization defect.
3. Run `mtgo-rs scenario minimize FAILURE.json MINIMAL.json`, then independently
   confirm that the reduced reproduction still demonstrates the same issue.
4. Add a permanent `tests/regressions/issue_...json` with issue, description,
   rules and known version/fix metadata. Label harness bugs explicitly.
5. Have the Rules Lawyer add an assertion that does not merely compare newly
   recorded production snapshots. Fix the implementation.
6. Intentionally refresh affected replay checkpoints only after independent
   assertions pass; never bulk-record a failing corpus to conceal divergence.
7. Run the PR campaign and relevant production mutants. Keep the original failure
   alongside minimized/fixed evidence in review artifacts.
8. Independently review evidence and update primitive metadata if thresholds are
   met. Record unresolved supported-behavior defects in the issue manifest.

CI currently archives failures and measured reports. It does not automatically
classify failures, authenticate reviewers, file issues or create regression PRs.
Client capture of full-session bug artifacts and that automation are subsequent
work. Nightly/weekly jobs request larger bounded campaigns rather than claiming
unmeasured coverage.
