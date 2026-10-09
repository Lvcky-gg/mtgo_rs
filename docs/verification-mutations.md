# Production mutation smoke verification

Run `python3 scripts/verify_mutations.py --output target/verification/mutations.json`.
The script copies the current workspace sources into a disposable temporary
workspace. Production files in the shared working tree are never mutated.
Every campaign uses a fresh disposable Cargo target namespace. `--target-dir`
selects its parent directory; prior or concurrent mutant artifacts cannot be
reused by another campaign. Copied source mtimes are also refreshed.

Each test must first pass on the unchanged baseline. Every production mutant must
compile. Compilation failures, timeouts, zero tests, and infrastructure failures
are errors rather than mutation kills. The JSON report includes exact commands,
test logs, counterexamples, mutation anchors, and the copied Rust-source digest.
The temporary workspace is removed on exit.

`python3 scripts/verify_gate_mutations.py --output NEW_REPORT.json` runs a separate
five-mutation campaign against the Python release gate and failure intake: accepting
truncated campaigns, unsuccessful exit codes, stale source evidence, substituted
failures, and exposing the original input to a faulty reducer. It uses independently
authored attacks, compiles each disposable production mutant, and requires a passing
nonempty baseline. Only exit1 with a final assertion-failure summary counts as a kill;
empty campaigns, import/compile errors, timeouts and abnormal exits fail accounting.
Each test family must pass its own nonempty unchanged control; sources are restored
between mutants so one mutation cannot contaminate a later baseline or kill.
Syntax compilation happens in memory and isolated test interpreters disable bytecode
writes, preventing stale cached modules from influencing later controls or mutations.
Weekly campaigns require both the Rust production matrix and this gate matrix.

The campaign deliberately targets these defects:

| Production mutation | Detection mechanisms |
| --- | --- |
| Automatic exhaustion returns an uncapturable error | Independent structured failure/report replay test |
| Replay ignores the scenario's advance budget | Independent failure-fixture test |
| An initial campaign failure invents a divergent action | Independent campaign/replay observation comparison |
| Reducer substitutes an earlier stale checkpoint | Independent minimizer oracle-preservation test |
| Game over retains priority | Independent terminal concession/winner golden and replay invariants |
| Report verifier trusts captured observations | Independent report tamper-rejection tests |
| Zero-action campaign skips initial normalization | Independent zero-action replayability test |
| CR 704.5f toughness `<= 0` becomes `< 0` | Rules golden test, generated property, independent decision-table differential |
| Opponent hand identities exposed by `PlayerView` | Hidden-information noninterference property |
| Shared command-zone casting ignores card ownership | Independently authored owner-only Commander golden test |
| Commander return confirmation acceptance is inverted | Independently authored graveyard/exile accept-and-decline golden test |
| A departed player's objects outside their hand stay in the game | CR 800.4a every-zone golden test |
| Control effects giving a departed player control persist | CR 800.4a control-effect golden test |
| A departed player's stack abilities are exiled as if they were cards | CR 800.4a stack golden test |
| A departed player stays in APNAP order | CR 800.4d trigger, "each player" token and "each opponent" count golden tests |
| A departing owner takes another player's ability off the stack | CR 113.7a/800.4a ability-outlives-owner golden test |
| Combat damage is still assigned to a departed player | CR 800.4e lifelink golden test |
| A permanent that lost all abilities keeps generating its static effects | CR 613.1 lord and CR 604.3 CDA golden tests |
| An ability-loss static removes its own effect | CR 613.6 global ability-loss golden test |
| Dynamic effects are matched against printed characteristics | CR 613.1e color-anthem and color-grant golden tests |
| Exact generic-mana payment incorrectly requires one extra resource | Explicit boundary and generated independent exhaustive allocation/payment checks |
| Shroud no longer blocks creature targeting | Independently authored keyword golden and generated targeting decision-table differential |
| Phased-out creatures become targetable | Independently authored phasing golden, generated targeting differential, and human-asserted scenario regression |

The spell-copy increment adds three compiled mutations: retaining the original
owner, accepting an illegal retargeting answer, and copying mana spent. Independent
copy goldens kill each. The fresh full campaign kills **19/19** curated mutants
with all unchanged baselines passing; `/tmp/mtgo-copy-final-mutants.json` records
Rust fingerprint `aa47e76c2bbba61d00b458fd4b56b922259bb11a92a38ef3f9c508048bd14ed4`.
This fingerprint matched the final working-tree Rust sources. Compile-interrupted
launches counted zero kills. This is a bounded smoke result, not a repository-wide
mutation score or confidence promotion.

The earlier layer campaign killed all sixteen compiled mutants. All three SBA mechanisms
failed independently, the hand-leak property failed, and the Commander ownership
golden tests failed. The CR 800.4 golden tests are Builder-authored (written from the CR text and seen failing first). This is a **sixteen-mutant smoke result**, not a claim about
repository-wide mutation coverage or a VERIFIED primitive. Controlled predicate demonstrations in `mutation.rs` are also kept
separate from production mutation evidence.

The fresh complete campaign observed **15/15 compiled mutants killed**, with
all unchanged baselines passing, at Rust-source fingerprint
`ad70722f5d125033d62b828785c36043428afac616d80607f1044acfb4e57aa1`
(report: `/tmp/mtgo-targeting-full-mutants.json`). This fingerprint covers Rust
sources, not a claim that every later working-tree revision has been assessed.

Separately, the scoped targeting campaign observed **2/2 kills**. Both the shroud
and phasing mutations fail an explicit golden and the independent generated
comparison. Removing the phasing exclusion also makes the permanent scenario
fail at **action 0**: production offers `Cast { object: ObjectId(2) }` where the
human priority assertion permits only passing or conceding. The fixture retains
those human choice/life/zone assertions alongside recorded replay checkpoints.
Scoped results are additional mechanisms for selected defects, not another
repository-wide mutation score or independent evidence for all fifteen defects.

Confidence metadata requires independently reviewed, same-build artifacts and a
larger production mutation sample. CI should archive the JSON report. Extending
this campaign should add a concrete production mutation and an independently
specified behavioral check; changing tests to follow the mutated implementation
invalidates the evidence.

The protection/prevention increment adds three controlled production defects:
protection blocking unpreventable damage, skipped shield-counter removal, and
skipped Phantom counter removal. All three fail independent goldens, the scoped
10k-case table, and the permanent human-counter-asserted regression. The protection
mutation also fails actual Engine combat. Fresh full campaign: **22/22 killed** at
Rust fingerprint `10205783fca5139ec33392ee53a475156a6c9ffa05a45470a3157e3b9650dcd1`
(`/tmp/mtgo-prevention-mutants.json`). Later concurrent compiler changes are outside
that snapshot's evidence. This is a curated smoke campaign, not a repository-wide
score or a VERIFIED primitive.

The Python gate mutation campaign now includes five regression-staging defects:
ignoring the review's scenario hash, accepting digest-only expectations, permitting
builder self-review, accepting a failed observed result, and ignoring changes to
the private input. Each uses the independently authored regression intake tests
and a separate passing unchanged baseline. Disposable copies include the existing
priority regression needed by those tests; no production or corpus file is mutated.
Compile/import errors do not count as kills. This is a curated harness-defense
sample, kept separate from Rust rules mutation evidence and confidence scores.

Shared-permanent mana verification adds three curated production mutants:
allowing two tap abilities on one permanent to supply two mana, recording an
ability that cannot make the selected color, and losing a colorless alternative
when colored and colorless abilities share one permanent. Independent grouped
exhaustive enumeration and direct goldens catch all three. The scoped campaign
kills **3/3 compiled mutants**, each with an unchanged passing baseline, at the
Rust snapshot recorded in `/tmp/mtgo-shared-mana-mutants-final.json`. This does not
rerun the other curated mutants or establish a repository mutation score.

The multiple-output increment adds two compiled production mutants: hiding
smaller alternative abilities and allowing a repeated same-color output to split
across colors. Whole-activation goldens and an independent generated exhaustive
oracle kill both. The sample is separate from the older shared-source campaign
and does not establish a repository-wide mutation score or confidence promotion.

Alternative-cost verification adds two curated defects: skipping the release of
optional Phyrexian mana assignments when mandatory generic payment needs them,
and truncating monohybrid costs after eight symbols. Independent symbol-assignment
properties and explicit boundary goldens catch both; the Phyrexian case also has
an actual Engine casting regression. These scoped samples do not establish a
repository-wide score or VERIFIED confidence.

Restricted-mana verification adds four curated production mutants: ignoring
floating restrictions, omitting provenance recording, retaining provenance after
step cleanup, and omitting provenance from the canonical diagnostic state. Each
must compile before an independent Engine test failure counts as a kill. These
samples measure the selected checks, not repository-wide mutation coverage.
