# CI evidence and release-judge verification

Independent attacks against a real successful PR report reproduced false acceptance
of missing generated replay checks, truncated later games/corpus fixtures, duplicate
checks, PASS with unsuccessful/missing/boolean exit codes, and explicit execution
errors. The gate previously inspected only the first check from a few families.

Campaigns now declare their complete sorted corpus inventory, requested game count
and expected check names before execution. The gate requires every planned check
exactly once with integer exit 0, no unsuccessful status and no error. Weekly plans
require both production Rust and Python gate mutations. Malformed inputs fail closed.

Measured reports include a SHA256 fingerprint of Rust/Python executable/test
sources, manifests/lockfile, toolchain/config files, stored property seeds, static
script test data, workflows and the scenario corpora. Campaigns reject source changes
during execution. The gate CLI independently compares the current source fingerprint
and corpus plan; older reports without that metadata must be regenerated. Schema
evaluation can receive explicit expected fingerprint/inventory for tests. Metadata
is not authenticated proof of execution; fabricated reports still require review.

Runner start errors and timeouts are recorded; malformed templates retain failed
reports. Nonempty output directories refuse overwrites. Both verification and release
test jobs initialize evidence before commands and tee formatting/Clippy/workspace
logs into always-uploaded artifact directories. Their Bash pipeline failures remain
failures. Independent workflow review caught an incorrectly nested release
initialization step during implementation; it was fixed before validation. Tool
installation logs remain in GitHub's job logs. Hosted workflows were not executed.

The controlled gate mutation tool uses disposable production copies and independent
attacks. Compiled mutations restoring truncated acceptance, unsuccessful exit
acceptance and stale source acceptance are all killed. Import/compile/timeouts,
abnormal exits, incidental failure text and empty mutation campaigns cannot count
as kills. Independent review found and fixed the empty-campaign false success.

Validation: 61 Python tests pass, one intentional skip; Rust formatting and diff
checks pass; both workflows parse as YAML. A fresh PR campaign passes all 29 checks
with 8 replays, 11 regressions and 800 observed semantic actions. Its CLI gate passes
against the current fingerprint. The strict closed-alpha gate correctly fails all
three unassessed release categories, preserving unknown counts as null. An earlier
passing report was correctly rejected after the mutation runner changed.

Final campaign and three-mutation evidence share source fingerprint
`4f8da71c8835497d38410b71e191075752ce269a16ec01c546c661b5535317b8`.
Rust production mutations and full workspace tests were not rerun this turn; Rust
engine code was unchanged. The prior source snapshots retain their own evidence.
The independent gate fixture retains historical check outcomes but explicitly uses
synthetic plan/provenance metadata for schema attacks; it is not a new campaign.
No primitive confidence promotion or release-readiness claim.
