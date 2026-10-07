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
cargo run -p mtg-verify --bin mtgo-rs -- confidence verification/primitives.json
PROPTEST_CASES=10000 cargo test -p mtg-verify
python3 scripts/verification_campaign.py --tier pr --output /tmp/verification-report
python3 scripts/verify_mutations.py --output /tmp/production-mutants.json
```

Output artifacts are additive: choose a new output filename. A failed scenario
exits nonzero and reports its first divergent answer, expected/actual canonical
state, SHA-256 digest and bounded JSON-path diff. Indices are zero-based. Recording
checkpoints establishes a replay baseline, not an independent rules oracle;
human-authored assertions and independently authored golden tests supply that
additional evidence.

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
The client does not yet capture/export every real session automatically.

## Independent checks shipped

* Golden CR 704.5f tests distinguish zero-toughness removal from destruction,
  including indestructible creatures with different owners/controllers.
* Generated properties check that boundary and ownership/zone conservation, and
  compare whole serialized player views after changing hidden opponent cards.
* A separate slow creature-SBA decision table compares production behavior across
  toughness, damage, deathtouch and indestructibility combinations.
* Canonical acceptance tests independently exercise allocation normalization,
  timestamp equality, zone order, private knowledge, RNG, suspended choices and
  replay divergence.
* Eight synthetic replays cover casting, multiple stack entries, enters triggers,
  combat, four-player priority, elimination, Commander casting and declined returns. They are not real alpha recordings.
* Four permanent regression fixtures record a confirmed canonical life-omission
  harness bug, the seed-484 eliminated-player priority engine bug and inconsistent
  command-zone representation and forced commander returns. Independent
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
  opponent-hand visibility, Commander ownership eligibility and inverted return
  decisions. Unchanged baselines must pass, mutants
  must compile, and named tests must fail before a mutant counts as killed.
  [Mutation details](verification-mutations.md) distinguish these from predicate
  smoke checks. Four mutants provide a milestone demonstration, not a general
  engine mutation score.

The first full PR campaign exposed the priority bug, which was independently
reduced from 26 answers to two passes with no objects. The fix skips eliminated
players during priority/ordinary-turn rotation, synchronizes the priority state
with its offered choice, and discards their queued extra turns. The subsequent
campaign passed 18 checks, six replays, two regressions and 800 semantic actions.
The Commander increment subsequently passed 20 checks with seven replays and
three regressions, and killed all three compiled production mutants.
The return-choice increment passed 22 checks with eight replays and four
regressions, and killed all four compiled production mutants.

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
[primitive registry](../verification/primitives.json) contains ten empty-evidence
primitives and explicit synthetic dependency graphs, all EXPERIMENTAL. Reviewer
names and artifact strings are advisory metadata, not authenticated approvals;
artifact review remains a human/independent-agent workflow. Automatic IR dependency extraction and
deck/client confidence presentation remain to be integrated.

`verification.json` deliberately marks state corruption, network divergence and
silent unsupported-rule assessments as **unassessed**. Empty lists cannot prove
zero known failures. PR gates reject observed failures and known blockers;
explicit closed-alpha release gates additionally reject missing assessments.
Use the Release workflow's `closed_alpha` input for the strict gate. Existing
ordinary releases retain their release mode and gain the verification campaign.
Repository branch protection must require **Verification / verify** to enforce
the PR merge restriction; a workflow file cannot configure branch protection.

Campaign reports distinguish requested budgets from measured results and archive
logs/seeds/reproductions. PR requests 10,000 cases per property and four semantic
games. Nightly requests one million cases and 100 games; weekly requests two
million and 1,000 games, plus production mutation checks. Scheduled structural
fuzzing uses 600/3,600-second bounded libFuzzer runs. These are initial budgets,
not claims that millions of games or 100 million transitions have been run.
Nightly/weekly campaigns have not been executed locally.

## Remaining expansion

The golden suite still needs direct propositions for the broader core actions,
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
Eliminated-player object/control-effect removal under CR 800.4 is also incomplete;
the elimination replays establish only their explicitly tested behavior.
