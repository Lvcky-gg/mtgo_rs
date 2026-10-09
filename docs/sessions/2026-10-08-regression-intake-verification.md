# Reviewed regression intake

Added `scripts/regression_intake.py` to prepare additive regression bundles from
already-correct scenarios. The tool requires issue/fix/rules metadata, a distinct
reviewer record bound to the exact scenario bytes, and direct behavioral
assertions. It validates a private copy using the existing scenario CLI, never
records new checkpoints, and preserves fixture/review bytes unchanged.

Successful output is PREPARED, not promoted or VERIFIED. Review identities are
recorded claims rather than authenticated identities; a human must still assess
whether review was independent and expectations are justified. Sidecars use
`.review`, avoiding the JSON scenario discovery surface, and participate in
source fingerprints. Fresh output is required; existing bundles are untouched.

Independent tests attack review metadata, candidate hashes, absent direct
assertions, execution status/exit-code disagreement, malformed observations,
and snapshot mutation. Existing corpus behavior and engine architecture remain
unchanged. PR validation retains eight replays and eleven regressions; staging
an existing fixture does not count as a new regression discovery.

Validation: 76 Python tests passed, one platform-specific test skipped. The
independently reviewed eliminated-player priority fixture passed real-engine
intake at `/tmp/mtgo-intake-reviewed-bundle`; its hash and bytes were preserved.
Five existing gate/intake-defense mutations were killed (the new staging module
has independent rejection tests but no claimed mutation score). Formatting and
diff whitespace checks passed. All 29 PR campaign checks passed, including the
8-replay/11-regression corpus and four semantic games. Closed-alpha assessments
remain unassessed, so confidence and release eligibility did not increase.
