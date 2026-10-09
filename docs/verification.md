# Agent-first verification

This increment establishes the scenario/replay foundation and the first independent
attack surfaces. It does not establish complete Magic correctness. The repository
audit and reused engine types are mapped in [verification-audit.md](verification-audit.md).

## Running it

```sh
cargo run -p mtg-verify --bin mtgo-rs -- scenario run tests/replays/basic_casting.json
cargo run -p mtg-verify --bin mtgo-rs -- replay tests/replays/multiplayer_elimination.json
cargo run -p mtg-verify --bin mtgo-rs -- scenario record INPUT.json OUTPUT.json
cargo run -p mtg-verify --bin mtgo-rs -- scenario fuzz INPUT.json OUTPUT.json
cargo run -p mtg-verify --bin mtgo-rs -- scenario minimize FAILURE.json MINIMAL.json
cargo run -p mtg-verify --bin mtgo-rs -- scenario report FAILURE.json BUG_REPORT.json
cargo run -p mtg-verify --bin mtgo-rs -- report replay BUG_REPORT.json
cargo run -p mtg-verify --bin mtgo-rs -- confidence verification/primitives.json
PROPTEST_CASES=10000 cargo test -p mtg-verify
python3 scripts/verification_campaign.py --tier pr --output /tmp/verification-report
python3 scripts/verify_mutations.py --output /tmp/production-mutants.json
python3 scripts/verify_gate_mutations.py --output /tmp/gate-mutants.json
python3 scripts/verification_gate.py --campaign /tmp/verification-report/report.json --output /tmp/verification-gate.json
```

Output artifacts are additive: choose a new output filename. A failed scenario
exits nonzero and reports its first divergent answer, expected/actual canonical
state, SHA-256 digest and bounded JSON-path diff. Indices are zero-based. Recording
checkpoints establishes a replay baseline, not an independent rules oracle;
human-authored assertions and independently authored golden tests supply that
additional evidence.

Minimization preserves the original failure message, the failing action's content
hash when applicable, and canonical diff paths. It rejects stale-checkpoint
substitution and never rewrites assertions. The CLI reports attempted and accepted
reductions and budget exhaustion; `campaign::minimize_with_report` exposes the
serializable report. It removes action chunks, conservatively removes setup objects,
and tries removing damage, tapped status and individual counters. Object removal
with remaining actions stays trailing-only to avoid rebinding object identities.
These reductions preserve an observable oracle, not causal equivalence: independently
review the Magic premises before accepting a regression. Deliberately failing
harness inputs live in `tests/minimization`, outside the passing engine corpus.

Bug reports reuse the original GameScenario and include the observed RunReport,
including the first divergent action, digest, expected/actual state and diff.
`scenario report` writes the attachment even when the replay fails, then returns
a failing exit status. `report replay` succeeds when it reproduces the captured
outcome exactly, including an observed failure: `reproduced: true` is not a claim
that the scenario passed. Altered observations or transitions fail verification.
Reports are additive, versioned and bounded at 64MiB; nested scenarios retain
their 16MiB validation limit. The structural fuzz entry point covers report parsing.
Attachments contain full private state and should use private bug-report channels.
This is an offline export from an existing scenario, not live UI recording or
automatic bug classification. Parse/setup errors and panics
remain errors rather than structured attachments. Rules review, fixing and
regression promotion remain separate steps.

Replay and semantic campaigns verify invariants at initial and terminal boundaries
as well as after answers. Zero-action campaigns normalize setup through the same
engine transitions as replay before capturing their final state.

Scenarios may specify `advance_budget` for the automatic transitions required to
reach each choice/terminal boundary. Existing artifacts default to 10,000 and omit
that field when serializing; smaller positive budgets are preserved in reproduction
and minimization. Exhaustion returns a structured failure with the actual stopped
state and digest, and no action index if it occurred before any answer. Recording
cannot bless an exhausted run. Semantic CampaignReport includes the same observable
RunReport, and the fuzz CLI prints it after saving the reproduction. These outcomes
can be exported and independently reproduced through replay reports.
The budget counts `Engine::advance` calls; it is not a timeout inside one call.
Small budgets can interrupt correct legal games. Review exhaustion before classifying
it as nontermination. Deliberately failing fixtures live in `tests/failures`.

## Representation and determinism

`mtg-verify` reuses `Engine`, `GameState`, `Answer`, `ChoiceKind`, `CardFace`,
`ZoneRef`, `ManaPool`, and engine RNG. JSON format version 1 stores build
provenance, a supported-rules version, embedded card IR and its hash, a seed,
initial setup, ordered answers and optional checkpoint assertions. It rejects
unknown versions, mismatched card hashes and malformed ownership/zone layouts.
The rules label `mtgo-supported-rules-v1` describes this engine's supported
behavior; it is not a claim of full compliance with a dated Comprehensive Rules
release.

The optional engine `verification` feature exposes a diagnostic snapshot without
making `GameState` a network serialization type. Canonicalization preserves all
state fields, private engine continuation state, pending choices, undo history,
rules-relevant event history, RNG, zone order and timestamp ordering/equality.
Object allocation gaps and timestamp offsets are normalized; ephemeral choice
and event correlation IDs are removed. Maps use sorted lossless key/value pairs,
including maps whose keys cannot be JSON object keys. Diagnostic artifacts are
full-information files containing hidden cards and RNG state. They belong in
private debugging/report channels, never opponent messages.

Version 1 reconstructs a bounded setup plus engine answers. It supports single-face
embedded card IR and cannot import arbitrary midgame continuation snapshots.
Initial stack entries must be established through recorded casting actions.
An action may set `expected_rejection: true` for intentionally invalid input. Replay
and checkpoint recording require the answer to fail, preserve the exact pending
choice ID, and leave canonical engine state unchanged. Unexpected acceptance fails
even during recording. A later action can retry the same choice with valid input.
Zone assertions may include `counters`, a map from existing engine counter kinds to
expected totals across matching objects. Missing counters contribute zero; totals
are checked during replay and recording with overflow-safe accumulation.


The client does not yet capture/export every real session automatically.

## Independent checks shipped

* Golden CR 613 layer tests (`crates/mtg-verify/tests/layers.rs`) cover P/T
  sublayer order against timestamps, characteristic-defining abilities, ability
  removal versus static effects, control changes under anthems and layer-6 timestamp
  order, using compiled card text. They found that a permanent which lost all its
  abilities still generated its static abilities' effects; fixed in
  `layers::effects`. They also found that "blue creatures get +1/+1"-style effects
  were matched against printed characteristics, so a color change (layer 5) never
  reached a later layer; `compute` now decides each dynamic effect's reach as its
  layer comes up, from the object's characteristics so far. Other objects referenced
  by a selector are still read as printed. Dependencies (CR 613.8) remain unimplemented.
* Golden CR 704.5f tests distinguish zero-toughness removal from destruction,
  including indestructible creatures with different owners/controllers.
* Generated properties check that boundary and ownership/zone conservation, and
  compare whole serialized player views after changing hidden opponent cards.
* A separate slow creature-SBA decision table compares production behavior across
  toughness, damage, deathtouch and indestructibility combinations.
* Nine independent core-action goldens check draw versus moving cards into hand,
  discard, mill, life/damage semantics, zero-event boundaries, failed-draw timing,
  and zone identity/ownership through actual effect resolution and SBAs.
* A second scoped oracle exhaustively enumerates mana-source color assignments
  and compares them with production matching. Generated checks independently
  validate activation/spending witnesses. Hybrid, phyrexian, snow, restrictions,
  activation costs and multi-output sources are outside this model; unsupported
  reference inputs return errors.
* Canonical acceptance tests independently exercise allocation normalization,
  timestamp equality, zone order, private knowledge, RNG, suspended choices and
  replay divergence.
* Eight synthetic replays cover casting, multiple stack entries, enters triggers,
  combat, four-player priority, elimination, Commander casting and declined returns. They are not real alpha recordings.
* Ten permanent regression fixtures record a confirmed canonical life-omission
  harness bug, the seed-484 eliminated-player priority engine bug and inconsistent
  command-zone representation, forced commander returns, phased-out targets, malformed
  input rejection, spell-copy ownership, payment history and identity, and
  unpreventable protection/shield/Phantom behavior. Independent
  golden coverage checks priority across consecutive eliminated seats; a separate
  acceptance test catches queued extra turns belonging to eliminated players.
* Independent Commander tests check owner-only shared-zone casting, forged-action
  rejection, tax affordability and actual second-cast payment. Return-choice tests
  cover accepting/declining graveyard and exile returns, owner/controller
  separation, no repeated prompt, reentry eligibility, APNAP decision order and
  simultaneous accepted moves.
* Structural generated tests and a cargo-fuzz target exercise real deck, compiler,
  scenario, IR, network-message and Scryfall ingestion boundaries with bounded
  inputs. SQLite saves, bulk ingestion and socket scheduling remain outside this
  target.
* Semantic campaigns select offered priority actions and use the existing policy
  for complex choices, then check stable-state structural invariants. Failures
  retain their setup, seed and ordered answers. This generator is not a legality
  oracle and currently does not vary every requested mechanic.
* Delta debugging removes actions and trailing initial objects while retaining
  the same failure message. Player/effect/choice shrinking is future work.
* Isolated production mutations change the actual zero-toughness comparison,
  opponent-hand visibility, Commander ownership eligibility, inverted return
  decisions, multiplayer departures and the exact generic-mana boundary.
  Unchanged baselines must pass, mutants
  must compile, and named tests must fail before a mutant counts as killed.
  [Mutation details](verification-mutations.md) distinguish these from predicate
  smoke checks. Sixteen mutants provide a milestone demonstration, not a general
  engine mutation score.

Direct `expected.zones` assertions use existing zone, card and player types and
are checked during both replay and checkpoint recording. Recording cannot bless
an incorrect human-specified count. Six fixtures now use these extra assertions.

The first full PR campaign exposed the priority bug, which was independently
reduced from 26 answers to two passes with no objects. The fix skips eliminated
players during priority/ordinary-turn rotation, synchronizes the priority state
with its offered choice, and discards their queued extra turns. The subsequent
campaign passed 18 checks, six replays, two regressions and 800 semantic actions.
The Commander increment subsequently passed 20 checks with seven replays and
three regressions, and killed all three compiled production mutants.
The return-choice increment passed 22 checks with eight replays and four
regressions, and killed all four compiled production mutants.
The CR 800.4 increments passed 22 checks and, with the mana boundary mutant, killed
all eleven compiled production mutants; one replay's checkpoints were intentionally
refreshed (see below). The mutation script now refreshes copied file times: copies
kept their original mtimes, so Cargo could reuse the previous run's last mutant from
the shared target directory as the next run's "baseline". Each campaign also uses
a fresh disposable target namespace; overlapping-run acceptance tests check that
prior/concurrent artifacts cannot contaminate its baseline.

The targeting increment passes 23 PR checks with eight replays, five regressions
and 800 observed semantic actions. Independent golden/reviewer tests and a scoped
10,000-case targeting oracle exposed selection and resolution defects; the fresh
compiled-production mutation matrix kills all fifteen curated mutants, including
shroud and phased-out selection. No confidence promotion follows from this smoke
result. See [targeting session](sessions/2026-10-08-verification-targeting.md).

The spell-copy increment passes 27 PR checks (eight replays, nine regressions,
800 observed semantic actions), 1,968 workspace tests, and a fresh 19-mutant
production smoke campaign. Independent goldens caught ownership, spent-mana and
prospective-identity defects. Scenario actions can assert rejection without state
or pending-choice changes. See [copy session](sessions/2026-10-08-copy-targeting-verification.md).

The prevention increment passes 28 PR checks (eight replays, ten regressions,
800 observed semantic actions), 2,007 workspace tests and a 22-mutant production
smoke campaign. Nine independent goldens and a scoped generated table check damage,
lifelink, counter side effects and finite prevention capacity. Zone assertions can
require counter totals even during checkpoint recording. See
[prevention session](sessions/2026-10-08-protection-prevention-verification.md).

The source of rules propositions is the [official Comprehensive Rules](https://magic.wizards.com/en/rules).
Independent agents authored rules/security tests, canonical acceptance tests and
differential/mutation checks separately from the scenario implementation.
The [agent handoff workflow](verification-agent-workflow.md) specifies role
deliverables and failure-to-regression review steps.

## Confidence and release policy

`confidence::PrimitiveEvidence` is serializable machine-readable evidence tied
to a build and artifact. VERIFIED requires independent review, golden evidence,
10,000 property cases, 1,000 differential cases, 10,000 fuzz actions and at least
20 actual production mutants with a kill rate of at least 95%. Evidence for other
builds and duplicate artifacts does not accumulate. Failures block promotion.
Cards inherit the minimum confidence of supplied dependencies; unknown/empty
dependencies are EXPERIMENTAL. Solver hooks count trajectory exposure and permit
competitive analysis only for VERIFIED trajectories. No primitive or card is
promoted to VERIFIED by this increment. The versioned
[primitive registry](../verification/primitives.json) contains twelve empty-evidence
primitives and explicit synthetic dependency graphs, all EXPERIMENTAL. Reviewer
names and artifact strings are advisory metadata, not authenticated approvals;
artifact review remains a human/independent-agent workflow. Automatic IR dependency extraction and
deck/client confidence presentation remain to be integrated.

`verification.json` deliberately marks state corruption, network divergence and
silent unsupported-rule assessments as **unassessed**. Empty lists cannot prove
zero known failures. PR gates reject observed failures and known blockers;
explicit closed-alpha release gates additionally reject missing assessments.
Campaigns declare the complete corpus/game/check inventory before execution. Gates
require every planned check exactly once, including generated replay checks and
weekly mutation checks, with integer exit 0 and no execution errors. Reports carry a
fingerprint of executable Rust/Python sources, Cargo/toolchain configuration,
workflow definitions, static test data and scenario corpora. The gate CLI independently
compares that fingerprint and inventory with the current tree. Changes invalidate
old evidence; legacy reports without a plan/fingerprint must be regenerated.
These consistency checks do not authenticate fabricated reports or establish rules
correctness. Independent tests/review and actual observed execution remain required.
Use the Release workflow's `closed_alpha` input for the strict gate. Existing
ordinary releases retain their release mode and gain the verification campaign.
Repository branch protection must require **Verification / verify** to enforce
the PR merge restriction; a workflow file cannot configure branch protection.

Campaign reports distinguish requested budgets from measured results and archive
logs/seeds/reproductions. PR requests 10,000 cases per property and four semantic
games. Nightly requests one million cases and 100 games; weekly requests two
million and 1,000 games, plus production and gate mutation checks. Scheduled structural
fuzzing uses 600/3,600-second bounded libFuzzer runs. These are initial budgets,
not claims that millions of games or 100 million transitions have been run.
Nightly/weekly campaigns have not been executed locally.
Campaign output directories must be empty; reruns cannot overwrite previous
evidence. Start failures, timeouts and malformed template data produce failed
reports. Sources changing during a campaign also fail it. Verification and release
workflows capture formatting, Clippy and workspace-test logs before the campaign;
always-upload retains them on early failure. Dependency/tool installation logs
remain available through GitHub's job logs. Hosted workflows have not been run here.

Failed corpus replays, semantic campaigns and generated replay checks trigger
bounded offline triage. It snapshots the input into a private directory, exports a
failure report, independently reproduces it, minimizes the snapshot, then captures
and reproduces the candidate. The candidate must retain the same failure message,
failing action content and canonical diff paths, as well as assertions/provenance
and non-object setup. Every command receives at most 30 seconds; at most five run.
Diagnostics and logs live under `triage_*` and are listed separately in
`failure_triage`. Successful triage cannot make the campaign/gate pass. Missing
artifacts, timeouts and changed failures retain diagnostics without an accepted
candidate. No fixture is automatically promoted: rules/causal review remains
required. Property-test failures still archive their own logs/shrunk seeds rather
than going through this scenario reducer.

## Remaining expansion

The golden suite still needs direct propositions for the remaining core actions,
layers, costs, replacement interactions, APNAP and multiplayer rules. Generated
invariants do not yet independently validate all targets, mana payments, SBAs or
network information boundaries. Broader independent models, actual alpha replay
ingestion, full-state reducers, automatic triage/PR creation, real-deck/network
impairment campaigns, mutation inventory expansion and confidence dashboards are
subsequent increments. Golden tests in `tests/rules/` and permanent scenarios in
`tests/regressions/` are the growth points.

The Commander command-zone representation mismatch and forced graveyard/exile
returns are fixed. The owner receives a CR 903.9a decision once per newly arrived
object; a declined object remains until another zone change makes it eligible
again. Hand/library replacement choices under CR 903.9b and broader Commander
interaction matrices still need implementation and independent verification.
A player leaving a multiplayer game now follows CR 800.4a: control effects giving
them control end and every object they own leaves the game (as it happens, not as
a state-based action), then their abilities and spell copies on the stack cease to
exist and anything still under their control is exiled. Their triggered abilities
are never put on the stack (800.4d). An ability on the stack is no one's property,
so a remaining player's ability outlives its source's departing owner (CR 113.7a).
Departed players are excluded from APNAP order, "each player" and "each opponent",
and combat damage isn't assigned to them (800.4e). Golden tests in
`crates/mtg-verify/tests/leave_game.rs` cover each step, concession with priority
and a mourned creature that another player owns. They were written from the CR
text and observed failing before the engine change, but by the same agent that
made it: they are Builder evidence, not independent Rules Lawyer review. The
`multiplayer_elimination` replay's checkpoints were refreshed after those tests
passed; the only state difference is the departed player's library leaving the
game. Codex's independent acceptance review
(`crates/mtg-engine/tests/leave_game_review.rs`) found the stack-ability ownership
defect, which is fixed. Still open: 800.4b (control changes, or tokens/permanents
arriving under a departed player's control, other than via "each player"), 800.4c,
800.4f–h choices and costs owed by departed players, and 800.4m effect durations.

Reviewed regression staging is available through `scripts/regression_intake.py`.
It binds an independent review record to the exact candidate bytes, rejects
fixtures with only recorded digests, and validates a private copy without
re-recording expectations. See `tests/regressions/README.md` for the review format.
A prepared bundle is not automatic corpus acceptance or VERIFIED evidence.

Mana verification now includes independently authored grouped-activation
enumeration: each physical permanent contributes at most one activation, while
its multiple one-mana tapping abilities offer alternative outputs. The oracle
compares feasibility and checks returned object/ability identity, selected
output, resource conservation, and exact payment. Ten thousand generated cases
cover up to four permanents with up to three abilities each, floating mana and
ordinary colored/colorless/generic costs. The direct G-or-C regression exposed
and fixed loss of the colorless alternative during production grouping. Hybrid,
Phyrexian, restricted/costly/repeatable abilities and multi-mana outputs remain
outside this oracle's scope; confidence remains experimental.

Multiple-output mana verification independently enumerates whole activations,
including fixed vectors, alternative abilities on one tapped permanent, and
uniform-color repeated choices. It validates feasibility and whether the
recorded activation identities and choices actually produce the spent mana.
Direct goldens preserve a counterexample where feasibility was true but the
returned plan was invalid. Production selects whole complex activations before
using its existing unit matcher; ordinary one-mana sources retain the matching
path. Capacity pruning rejects impossible costs without enumerating every choice
and keeps life-paying Phyrexian choices available. Two real-casting regression
fixtures retain the discovered GG/C and repeated-choice defects. Generated scope
remains bounded to unrestricted fixed outputs and plain mana costs; independent
pruning boundary tests cover Phyrexian life payment separately.

Alternative-cost verification independently enumerates mana-symbol assignments
for ordinary, hybrid, monohybrid and Phyrexian costs, then checks both feasibility
and the returned plan's mana/life payment. It exposed optional Phyrexian matching
starving mandatory generic payment, plus truncation of monohybrid alternatives.
Production now reserves mana for mandatory generic symbols by paying optional
Phyrexian symbols with life when required; all monohybrid symbols are considered
with prefix feasibility and failed-allocation memoization. Independent direct
checks include costs exceeding eight symbols and the machine word size, exact
life boundaries, and a permanent Engine regression. Generated scope remains
ordinary alternative symbols and unrestricted one-mana tapping sources; snow,
restricted activation and other unsupported behaviors are excluded.

Restricted floating mana now retains source provenance and conjunctive spending
conditions through automatic activation and later payments. Independent Engine
goldens and generated capacity comparisons cover ordinary typed restrictions,
mixed restricted/free pools, ability exceptions, player isolation, nested
restrictions, canonical digests, and step cleanup. The leftover-mana leak has a
permanent regression. This scoped primitive remains EXPERIMENTAL; arbitrary
dynamic predicates and manual restricted activations are outside this coverage.
Evidence fingerprints include the issue manifest and confidence registry, so
changing verification policy invalidates earlier campaign evidence.
