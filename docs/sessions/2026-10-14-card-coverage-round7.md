# Card coverage, round 7 — 2026-10-14

Using the same `/tmp/card-coverage.sqlite` snapshot, coverage rises from 17,678
to 17,690 / 34,914 (50.7%). The complete card-name comparison gains 12 cards
and loses none. The existing five-card name-lookup difference remains
(17,695 versus 17,690).

## Life-history conditions

Added `Condition::YouLostLifeThisTurn` using the existing per-turn life-loss
history. The compiler recognizes `you lost life this turn`, `you've lost life
this turn`, and combined `you gained or lost` / `you gained and lost` conditions.
The combined forms use the existing OR and AND conditions. They check actual
gain/loss events, not the net life change; zero life gain does not qualify.
The existing turn reset clears the history without new state metadata.

Newly playable: Essence Channeler; Lunar Convocation; Star Charter;
Starlit Soothsayer.

## Combined activation restrictions

The compiler accepts `Activate only … and only if …` by compiling the leading
timing or frequency restriction with the ordinary activated-ability parser, then
adding the condition to its activation cost. Timing and frequency restrictions
remain independent of the condition and retain their existing engine checks.

Newly playable: Arcum's Sleigh; Cabal Inquisitor; Chronatog Totem; Coffin Puppets;
Dread Wanderer; Groundling Pouncer; Gutterbones; Kuldotha Phoenix.

Six compiled gameplay regressions cover OR/AND life histories, net-zero changes,
another player's life loss, zero gains, loss-dependent static keywords and turn
reset, Groundling Pouncer's opponent filter and once-per-turn limit with mana
available, Gutterbones' own-turn restriction alongside actual opponent life loss,
and Dread Wanderer's hand-size condition alongside sorcery timing. Dread Wanderer
is unavailable while a spell is on the stack even after its hand-size condition
becomes true; it returns tapped when both requirements are satisfied.

Validation: targeted compiled tests; `cargo test --workspace --quiet` including
the replay/regression corpus; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all --check`; and `git diff --check`. Workspace tests run outside
the sandbox for their existing loopback networking tests. Changes remain uncommitted.
