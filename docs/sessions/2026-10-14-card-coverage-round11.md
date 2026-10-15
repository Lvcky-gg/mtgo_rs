# Card coverage, round 11 — 2026-10-14

Using the same `/tmp/card-coverage.sqlite` snapshot, coverage rises from 17,744
to 17,746 / 34,914 (50.8%). The complete card-name comparison gains Kadena's
Silencer and Summary Dismissal, with no losses. The existing five-card
name-lookup difference remains.

The parser recognizes bare plural abilities and collective stack selectors for
countering or exiling all matching objects. Controller filters remain relative
to the effect's controller, and `other` excludes the resolving spell. Existing
targeted counterspell handling remains intact.

A compiled gameplay regression exercises four cases with both players' triggers
and a spell on the stack: counter all abilities, counter opponents' abilities,
counter opponents' spells, and Summary Dismissal's combined exile/counter effect.
It verifies which objects resolve through their life-gain effects.

Validation: targeted compiled gameplay test; complete coverage comparison;
`cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all -- --check`; and `git diff --check` pass.
`cargo test --workspace --quiet` also passes, including existing loopback
networking tests run outside the sandbox. Changes remain uncommitted.
