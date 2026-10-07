# Optional target controls

Claude's round-13 handoff identified that the GUI could not decline an optional
target slot. This blocked finishing `any number of target ...` announcements,
and forced a choice for `up to one target ...` while legal targets were present.

## Behavior

Target prompts now carry serde-defaulted per-slot `optional` metadata. The
engine supplies this directly from printed target specs, independently of its
fallback answer: the first optional slot can default to a target and still be
legally declined. Casting during resolution supplies the metadata too. Copy
retargeting keeps slots required because it preserves existing target counts.

The GUI renders `No target` for optional slots. A single-slot prompt immediately
submits an empty target answer. Multi-slot prompts allow optional slots to stay
empty and still require every nonempty mandatory slot to have a legal selection
before `Confirm targets` is enabled. Clearing an optional selection also removes
it from the selection undo order. Ordinary target picks retain their immediate
single-slot behavior.

Missing metadata defaults to required, preserving the behavior of older JSON
prompts. Pattern matches and test/example initializers elsewhere received only
mechanical adjustments for the added field. This batch improves gameplay access
for already-compiled cards; it does not change printed-text coverage.

## Validation

Five new UI/serialization regressions check skipping despite a nonempty fallback,
mixed required/optional confirmation and clearing, mandatory controls, illegal
selection rejection, and legacy JSON defaults.

Two new printed-card gameplay regressions in `compiled/optional_target_prompts.rs`
check optional versus mandatory enforcement and ending an any-number run before
or after the first target. Both first and later run prompts report optionality,
despite their different defaults.

Passed:

- `cargo test -p mtg-app -p mtg-oracle -p mtg-engine -p mtg-policy --offline --quiet`
  (1,323 passed, 43 existing tests ignored; includes both full network-match tests).
- `cargo test -p mtg-oracle --test compiled optional_target_prompts --offline --quiet`
  (both regressions passed after adding the any-number case).
- `cargo clippy -p mtg-app -p mtg-engine -p mtg-oracle -p mtg-policy --all-targets --offline -- -D warnings`
- `cargo build -p mtg-app --bin mtg-gui --offline --quiet`
- `git diff --check`

The first sandboxed app-suite run hit loopback socket permission errors. The
combined suites were rerun with the approved `cargo test` permission and passed.
