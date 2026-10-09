# Independent spell-copy verification

Nine new golden tests in `crates/mtg-verify/tests/copy_targeting_rules.rs`
drive actual Engine casting, X/mode/target announcements, copy spells,
responding exile spells, priority passes, and resolution. Expectations were
independently checked against official Wizards Comprehensive Rules 707.10,
707.10c, 115.3, and 608.2b.

The initial baseline confirmed a genuine ownership defect: P1's copy of P0's
spell was controlled by P1 but retained owner P0. CR 707.10 requires the copy's
owner to be the player under whose control it enters the stack. The Builder
fixed the production copy transition; the independent ownership assertion was
not weakened.

A second independent baseline exposed copied mana-payment history. An X=3
spell paid with three green mana gains life equal to green mana spent. The
original correctly gained three life, but its copy also gained three instead
of zero. The CR 707.10 Dawnglow Infusion example explicitly distinguishes mana
from objects referenced by copied costs. The golden requires retained X and
mode, six total damage, three life for the original controller, no life for
the copying controller, and empty object/context mana-payment histories.
After the Builder cleared both copied payment histories, all nine independent
goldens passed. The original still pays three green mana and gains three life;
its copy retains X=3 but gains zero life.

The permanent `issue_local_copied_spell_owner.json` regression casts the original
and a copy spell through six real engine answers. Independently authored stack
zone assertions require one original owned by P0 and one copy owned by P1,
including while recording checkpoints. The focused `seed_copy_owner` example
maintains this fixture without altering attack-agent fixtures.

Coverage includes copied X=3, mode and targets, new owner/controller, copy as
damage source, no additional cast event/count, and ceasing to exist after
resolution. Retargeting changes only the copy; preserves two actual targets
and an unchosen optional slot; rejects new illegal targets and duplicates within
one two-target occurrence while keeping the decision pending; permits one
object to be reused across separate target occurrences; and permits
retaining an existing illegal target, whose all-illegal copy still fails to
resolve under CR 608.2b.

The corrected retargeting interface excludes unchosen placeholders from its
offered groups. The test driver now submits two actual target groups rather
than a third empty placeholder group. It still independently asserts preserved
empty-slot metadata, cardinality, and unchanged original targets.

Seed-owned replay/regression checkpoints were refreshed with the new frozen
target-group and pending-resolution metadata. Hand-authored life/zone assertions
and issue metadata remain in the seeding tool. Attack-agent fixture files are
not regenerated or overwritten by that tool.

These tests establish scoped verification evidence. Registry confidence remains
EXPERIMENTAL; no primitive or card is promoted. Copies of abilities, alternative/additional costs, permanent
copies, copy-trigger interactions, and wider modal/cardinality matrices still
need independent coverage.

Eight independent Breaker tests in `mtg-engine/tests/copy_targeting_review.rs`
confirmed the original-identity exclusion defect and now check prospective copy
ownership, allowing the original spell as a new target, duplicate restrictions,
retained illegal targets, unpublished preview state, and identical state/event
logs after eight rejected attempts versus a clean run. A temporary prospective
copy is evaluated in a cloned state; the real stack is unchanged until the
retargeting decision is complete.

`ScenarioAction.expected_rejection` enables permanent malformed-input artifacts.
Both replay and recording require rejection, unchanged canonical state, and the
same pending choice ID. Five independently authored acceptance tests cover the
new boundary. Unexpected acceptance cannot be blessed by checkpoint recording.

Three spell-copy JSON regressions preserve ownership, mana-spent life totals
(23/20 after both spells resolve), and prospective identity plus a rejected
malformed retargeting answer. The identity fixture's recorded candidate set was
independently checked to contain the original spell and its final copy target
was explicitly checked to equal that original. Existing checkpoint refreshes
also incorporate concurrent noncreature-spell tracking; they preserve authored
rules assertions rather than supply new rules expectations.

Final validation: 1,968 workspace tests pass, 44 are ignored across 84 suites;
formatting, workspace Clippy and diff checks pass. Python harness tests pass
(18 tests, one skipped). PR campaign passes 27 checks with eight replays, nine
regressions, and 800 observed semantic actions. The fresh production campaign
kills 19/19 curated mutants. Its source fingerprint matches the working tree:
`aa47e76c2bbba61d00b458fd4b56b922259bb11a92a38ef3f9c508048bd14ed4`.
Earlier compile-interrupted launches counted zero kills and are not evidence.
These are bounded smoke results, not repository-wide correctness or mutation
coverage. Retargeting filters depending on newly chosen other targets and the
broader copy interactions listed above need further independent coverage.
