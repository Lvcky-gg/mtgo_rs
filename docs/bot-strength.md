# Bot strength: from heuristics to a solver

Goal: an opponent in the class of a chess engine or a poker solver, and — from the
same machinery — deck recommendations scored by simulated matchups.

## The rule

**Nothing counts as stronger until the arena says so.**

```text
cargo run --release -p mtg-app --example arena -- --seeds 16            # searcher vs heuristic
cargo run --release -p mtg-app --example arena -- --seeds 4 --baseline  # heuristic vs itself, timing
```

Every seed is played twice with the seats swapped, so both bots get the same deck,
shuffle and play/draw; the score is the mean over seed pairs, with a 95% interval and
Elo. A change that does not move the interval has not been shown to help.

## Where it stands (2026-10-16)

| Piece | State |
|---|---|
| Heuristic `Bot` (`mtg-policy/src/bot/`) | Codex-owned; threat scores, responses, combat |
| `Engine: Clone` | done — search forks the game |
| Determinization (`search/determinize.rs`) | hidden hand+library cards re-dealt per playout, RNG reseeded; knows deck lists |
| Static evaluation (`search/eval.rs`) | hand-weighted material → logistic win probability |
| Search (`search/mod.rs`) | flat Monte Carlo: candidates × heuristic playouts to a turn horizon, UCB1, common random numbers |
| Candidates | priority actions, attack/block plans, targets, modes, optional effects |
| Arena (`mtg-app/examples/arena.rs`) | paired seeds, Elo ± CI, adjudication at a turn limit |
| Engine speed | 6× fewer instructions per game (38.1G → 6.3G, one commander game) |
| Learned evaluation | `tune_eval` example fits weights by logistic regression; held-out log loss 0.605 → 0.515 |
| Deck pool (`deck_pool` example) | 120 published MTGO lists ≥90% playable (103 Pauper); arena `--pool pauper` samples pairings |
| State encoder (`search/encode.rs`) | schema v1: 39 global features + per-object tokens (112 features from characteristics and hashed IR words, slot, name hash); opponent hand is a count |
| Self-play recorder (`selfplay` example) | shards under `~/.local/share/mtgo_rs/selfplay/v1/`; first run: 28,283 Pauper games, 323k positions |
| Value network (`search/value.rs`, `tools/learn/train_value.py`) | held-out log loss **0.441** (78.8%) vs 0.573 (68.5%) for logistic on global features; Rust matches PyTorch to 1e-7; 123 µs per evaluation; arena `--value PATH`: same strength as the linear eval, 33% cheaper per decision (see below) |

**First measurement (2026-10-16):** searcher (64 playouts, horizon 1 turn, racing, keeps the
heuristic's answer unless beaten by a paired 2σ margin) vs heuristic bot on Demo/Weirds,
8 seeds × 3 pairings × both seats: **61.5% ± 8.3%, +81 Elo [+22, +145]**, 69 overrides in
10,385 searched decisions, ~120 ms per decision. Fitted evaluation weights: 63.5% ± 8.3% —
not distinguishable yet, so the defaults stand. At 16 playouts the margin rule never fires
(too noisy) and the searcher equals the heuristic.

**Pauper pool (2026-10-16):** 32 sampled pairings × both seats, 64 playouts, horizon 1,
turn limit 60, 30-minute game limit. Searcher with the linear eval: **73.4% ± 8.8%, +177 Elo
[+105, +266]**, 960 ms per decision, 3,600 overrides in 20,758 decisions. With the value net:
**71.9% ± 8.7%, +163 Elo [+94, +247]**, 648 ms per decision, 2,587 overrides. Paired by seed,
net − linear = −1.6% ± 9.1%: no strength difference yet, but the net makes search a third
cheaper (racing settles sooner on less noisy leaves). A 5-minute game limit adjudicated most
games — searching every decision of a 20-turn Pauper game takes minutes — so search cost,
not loops, is the arena's bottleneck.

Lessons: without the margin rule, flat MC overrides between equivalent moves at random
(land before or after a spell, or skipping the land) — pure noise. A game that loops within
the turn limit plus a search on every decision runs for hours; the arena caps choices at 5,000.

## Learning plan (agreed 2026-10-16)

Human game logs are not the training source: MTGO publishes decklists and finishes, never
move-by-move games; tracker logs are private, and 17lands' public data is per-game draft
summaries. A human log is also only usable if this engine can replay it move by move, which
half of all cards cannot yet. Self-play gives unlimited games in the engine's own state format,
exact outcomes, and the hidden cards as labels. Scraped data is used where it is strong:

- **Deck pool** — published lists (all formats on mtgo.com/decklists), filtered to playable
  cards, so self-play and the arena cover a real field instead of five decks.
- **Opponent priors** — archetype → likely cards, for hand inference.
- **Per-card statistics** — later, as evaluation features.

Phases, each gated by held-out loss and then the arena:

0. **Deck pool** — `deck_pool` example → `~/.local/share/mtgo_rs/selfplay/pool.json`;
   `arena`/`tune_eval` take `--pool`.
1. **State encoder** (`search/encode.rs`) — a fixed tensor per position from one player's
   view: global scalars, each card as a vector derived from its compiled IR (works for unseen
   cards), zones as sets of card vectors. The opponent's hand is a count only.
2. **Self-play recorder** — shards of (encoded state, candidates, search visit counts, final
   result, true hidden cards as labels only).
3. **Value network** — trained in PyTorch (CPU, or ROCm on the RX 7600), weights exported,
   evaluated in dependency-free Rust; replaces `eval::evaluate` at playout leaves.
   Gate: held-out log loss below the linear eval's 0.515, then an arena win.
4. **Policy network** — scores candidates; priors for PUCT allocation and the playout policy.
5. **Expert iteration** — search with the nets, train policy on visit counts and value on
   outcomes, repeat; each generation must beat the last in the arena.
6. **Deck recommendations** — the value net as a fast matchup estimator over card swaps,
   confirmed by the arena.

## Roadmap

1. **Measure and tune.** More seeds and the commander decks; tune budget, horizon and
   margin; sequential testing (SPRT) to accept changes; adopt fitted weights only on an
   arena win; refit on more decks (five decks overfit: mill makes "library shortfall" −2.9).
2. **Engine speed.** Playouts are the cost of everything below. Next: a battlefield
   index or a per-generation effects memo (needs the direct `objects`/`zone` mutation
   sites to cooperate), then the `legal_actions` / mana planner path.
3. **Learned evaluation.** Fit `Weights` to self-play outcomes (logistic regression on
   features → who won), then richer features (evasion, per-card value from win-rate
   statistics, mana open, threat scores), then a small network — the NNUE step.
4. **Deeper search.** Information-Set MCTS over the searcher's own decisions, macro-actions
   (cast X targeting Y as one move), progressive widening for large action sets.
5. **Hidden information.** Infer the opponent's hand from what they did and did not play,
   with a deck prior from metagame data (`docs/tournament-data.md`) — the "range" of a
   poker solver. Mixed strategies where being predictable is exploitable (attacking into
   open mana, holding up a counter).
6. **Deck recommendations.** The arena over deck variants: score a card swap by its
   simulated matchup win rate against a field.
