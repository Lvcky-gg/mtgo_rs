# Restricted floating mana verification

Independent Engine reproduction exposed a rules defect: a creature-only source
producing C2 paid C1 for a creature, then its leftover C1 could incorrectly pay
for an instant. The independent verifier specified observable expectations before
recording `issue_local_restricted_mana_leftover_leak.json`.

The Builder preserved restricted mana as GameState provenance carrying player,
source, amounts and conjunctive conditions. Payment planning excludes ineligible
floating amounts, spending consumes authorized provenance, and step cleanup
clears it. Nested wrappers combine their restrictions. The diagnostic state
includes nonempty provenance while retaining existing unrestricted fixture bytes.
Structural invariants reject unknown players, empty records and restricted totals
exceeding the aggregate pool. Source existence is not required: mana can outlive
its source.

The separate verifier supplied 14 tests including 10,000 generated Engine cases.
Coverage includes mixed free/restricted pools, Powerstone-style ability exceptions,
ordinary typed predicates, nested restrictions, player isolation, canonical
differences, malformed provenance and cleanup. Arbitrary dynamic predicates,
manual restricted activation and aggregate ManaPool u16 overflow remain outside
this increment. The latter is a preexisting issue, not repaired by this change.

Adding the scoped primitive exposed a bootstrap assertion fixed at 12 primitives;
it now expects 13 and still requires every primitive/card to be EXPERIMENTAL.
Independent Python cases also exposed missing policy inputs in evidence hashes.
The fingerprint now includes verification.json and verification registry JSON,
so changes to those inputs invalidate earlier evidence.

Validation used immutable `/tmp/mtgo-restricted-mana-source-snapshot` because
concurrent compiler/card work changes the live workspace. Snapshot evidence:

- Full workspace: 2,112 passed, 44 ignored, zero failed across 96 suites.
- Formatting and workspace/all-target Clippy with warnings denied: PASS.
- Python policy suite: 79 tests, one skipped, PASS.
- PR campaign: all 34 planned checks PASS; eight replay and 16 regression fixtures;
  10,000 requested cases per property and 800 semantic actions observed.
- Normal evidence gate: PASS; closed-alpha blocking assessments remain unknown.
- Four selected compiled mutants killed, with passing baseline and 1,000 cases
  per mutation check: ignore restrictions, omit recording, omit cleanup, omit
  provenance from digest. This is not a repository-wide mutation score.

Campaign fingerprint:
`d5848e5edb4ecac08bd6ea96f29e7f0f6dd9b351d88d0ae88c2d60caa19d7f40`.
Mutation Rust hash matches the snapshot:
`96f7f8b71485c75d73040f62e8bafd8d8916c60b501de3d09dab3c5ff44faf78`.
Reports are `/tmp/mtgo-restricted-mana-snapshot-pr-final/report.json`,
`/tmp/mtgo-restricted-mana-snapshot-mutants.json`, and
`/tmp/mtgo-restricted-mana-snapshot-gate.json`.

No confidence promotion, release-readiness claim, or validation of later live
compiler/card changes is made.
