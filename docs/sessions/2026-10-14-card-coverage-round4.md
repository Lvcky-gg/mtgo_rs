# Card coverage, round 4 — 2026-10-14

Using the same `/tmp/card-coverage.sqlite` snapshot, coverage rises from 17,644
to 17,654 / 34,914 (50.6%). The complete card-name comparison gains ten cards
with zero losses. The name-check command retains its existing five-card lookup
difference (17,659 versus 17,654).

## Poison conditions

Added `Value::MostPoisonCounters`, which evaluates the largest poison count among
selected players, with zero for an empty selection. The compiler uses it for
`an opponent has N or more poison counters` and `an opponent is poisoned`.
The latter means at least one poison counter. Opponents use the existing living
player selector, excluding the controller and players who have left the game.
Different opponents' counters are never summed. The value participates in the
IR visitor; its ordering footprint remains conservative, like poison effects.

These conditions use the existing paths for static abilities, intervening-if
triggers, cast-cost reductions, activation restrictions, and mana planning.

Newly playable: Apostle of Invasion; Bonepicker Skirge; Chittering Skitterling;
Distorted Curiosity; Glistening Sphere; Incisor Glider; Ravenous Necrotitan;
Sinew Dancer; Viridian Betrayers; Vivisection Evangelist.

Seven compiled gameplay regressions check:

- A single living opponent must meet the threshold; two opponents with two
  counters each do not qualify, and the controller's counters do not qualify.
- Static bonuses update after poison changes and disappear when the only
  qualifying opponent leaves the game.
- The one-counter poisoned condition grants infect and changes combat damage
  into poison rather than life loss.
- Distorted Curiosity costs one blue mana at three poison but remains
  unaffordable with that mana below the threshold.
- Glistening Sphere's three-mana ability is available manually and to automatic
  payment only at the threshold; both floating and automatic payment work.
- Vivisection Evangelist rechecks its condition after the qualifying opponent
  leaves. A different qualifying opponent preserves the effect; with none,
  the effect does nothing even though its target remains legal.
- Vivisection Evangelist does not trigger below the threshold.

Validation: targeted compiled tests; `cargo test --workspace --quiet` including
the replay/regression corpus; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all --check`; and `git diff --check`. Workspace tests run outside
the sandbox for their existing loopback networking tests. Changes remain uncommitted.
