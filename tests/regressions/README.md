Every confirmed engine bug must acquire a minimized permanent JSON scenario
before being considered fixed. Name fixtures `issue_0481_clone_blood_moon.json`
and fill metadata issue, description, rules, first_affected (if known), and
fixed_in. The shared corpus test automatically discovers every JSON here.

The initial fixture `issue_local_canonical_life_omission.json` records a genuine
verification-subsystem bug: player life was omitted from the diagnostic state,
so distinct life totals initially produced identical canonical digests. Its
companion corpus test changes life and requires divergence after the first
answer, without relying on a separate explicit life assertion.

This is a harness regression, not an invented historical Magic-engine issue.
Harness and rules-engine regressions must be distinguished in reports.
Regression count must not decrease without an explicit rules-version migration.

`issue_local_terminal_priority.json` records a legal two-player concession after
which the departed player incorrectly retained priority. Initial and terminal
invariants now run even without another choice. The independent boundary golden
also checks that no player holds priority once the game has ended (CR104.1).

`issue_local_priority_eliminated_player.json` is a confirmed engine regression
discovered by semantic campaign seed 484, then independently reduced to two
passes and no zone objects. Its golden companion asserts that the next surviving
player receives both the priority choice and the rules-state priority holder.

`issue_local_shared_command_zone_casting.json` records the confirmed engine
representation mismatch between shared Command zones and player-keyed casting
lookup. Independent golden tests also cover owner-only permission, the tax
affordability boundary, and shared command-zone return destinations.

`issue_local_forced_commander_return.json` records the confirmed suppression of
the owner's optional CR903.9a decision. Its minimal fixture has one lethal
commander and one owner-declines answer; independent golden tests assert the
declined card remains in the graveyard and is not repeatedly offered.

`issue_local_phased_out_targeting.json` records the phased-out targeting defect
found and minimized by an independently authored rules decision table and
property generator. Its priority checkpoint is a human rules assertion: a
required-target spell cannot be cast when the sole battlefield creature is phased
out. No production-recorded checkpoint supplies this expected behavior. Life and
zone assertions additionally require the creature and spell to remain in place.
The explicit golden, generated property, and this scenario independently fail
when the production phased-out exclusion is removed.
Recorded canonical state/digest checkpoints additionally verify replay determinism;
the manually specified priority, life, and zone assertions are preserved and
were checked for agreement before accepting those recorded checkpoints.

`issue_local_malformed_target_rejection.json` deliberately records six invalid
answers to one target prompt: wrong answer kind, missing outer slot, missing
required target, unknown object, duplicate/oversized group, and extra outer slot.
Each action explicitly expects rejection. The same prompt must survive, and a
subsequent valid target remains usable. Human target-choice, life, and zone
assertions are preserved alongside recorded deterministic checkpoints. Recording
cannot convert an unexpectedly accepted invalid answer into a passing replay.

`issue_local_copied_spell_owner.json` requires one original spell owned by P0 and
one copy owned by P1 on the stack (CR112.2/707.10).
`issue_local_copied_mana_spent.json` requires life23/20 after a green-paid X=3
spell and its copy resolve: the copy retains X but no mana was spent on it.
The `seed_copy_owner` example maintains both fixtures with authored assertions.

`issue_local_copied_spell_target_identity.json` requires a copy to be able to
newly target the original spell (CR115.5/707.10c). It first records rejection of
an illegal player target while keeping the pending choice unchanged. Its expected
candidate set includes the original, and its final copy targets that original.

`issue_local_unpreventable_prevention.json` reproduces three confirmed damage bugs
in one small real-casting scenario. Untargeted red damage cannot be prevented,
protection cannot suppress it, and shield/Phantom counter-removal effects still
apply. Independent life23/20 and battlefield counter totals1/1 are checked even
while recording; state checkpoints additionally preserve exact replay behavior.

Reviewed regression intake stages an already-correct scenario without recording
new expectations:

```bash
python3 scripts/regression_intake.py --scenario candidate.json \
  --review candidate.review --output /tmp/new-regression-bundle
```

The review is JSON with `format_version: 1`, distinct nonempty `builder` and
`reviewer`, `scenario_sha256` (SHA-256 of the candidate's exact bytes),
`expected_behavior`, `evidence`, and `kind` (`engine` or `harness`). Candidate
metadata must identify the issue, description, rules, and fix. At least one direct
life, zone, choice, or rejection assertion is required; recorded digests alone
cannot establish a regression's expected Magic behavior.

The tool runs a private copy through `scenario run`, requires an observed pass,
and preserves candidate and review bytes in `issue_*.json` and `issue_*.review`.
The `.review` extension keeps review records outside the JSON scenario corpus;
review records are included in campaign source fingerprints. Copy the prepared
fixture and review into this corpus only after review. Intake does not insert
files into the corpus, authenticate reviewer identities, or promote confidence.
Its review fields document independent judgment rather than proving it.

`issue_local_mana_colorless_alternative.json` preserves the confirmed G-or-C
shared-permanent defect. A one-colorless-cost gain-three instant must be castable
using the colorless ability, tap the permanent once, and resolve to life23 with
the instant in its owner's graveyard. Independent life and zone assertions were
specified before recording checkpoints. The associated independent Engine golden
additionally checks exactly one colorless mana produced and spent, an empty pool,
and the tapped permanent. Grouped exhaustive properties check returned ability
identity and prevent double activation of one permanent.

`issue_local_mana_larger_output_hides_colorless.json` confirms a larger GG
ability cannot hide a C ability on the same permanent. A C-cost spell remains
castable and resolves to life23. `issue_local_mana_repeated_choice_cannot_split.json`
requires a WU-cost spell to stay uncastable when the only source makes two mana of
one chosen color. Each fixture preserves independently specified life/zone and
cast-availability expectations. Independent Engine goldens also inspect actual
activation/payment events and the tapped or untapped source, respectively.

`issue_local_mana_generic_phyrexian_competition.json` requires a `{1}{G/P}`
instant to remain castable with one green source and available life. The source
pays the generic symbol and two life pays the Phyrexian symbol. Independent
expectations require life21 after gain-three resolution and the correct final
zones. The Engine golden independently checks life18 immediately after payment,
a tapped source, an empty mana pool, one G produced/spent and life delta -2.

`issue_local_restricted_mana_leftover_leak.json` records a restricted source
producing two mana: casting an eligible creature spends one, while the remaining
mana must not pay for an ineligible instant. Expectations were independently
specified before recording the deterministic checkpoints.
