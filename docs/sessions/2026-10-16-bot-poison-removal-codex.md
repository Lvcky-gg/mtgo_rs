# Poison-aware removal urgency

The shared threat score now separates incoming player life damage from poison.
Infect contributes poison instead of life loss; toxic adds poison per positive
combat hit, including double strike. Only attackers contributing to a lethal
resource receive its urgency bonus, so a small lethal poison attacker outranks
a larger ordinary attacker whose damage the player can survive. A zero-power
toxic attacker receives no poison urgency.

The shared score is used by proactive removal selection and target selection.
Six compiled-card game scenarios check actual removal targets and survival:
lethal infect, lethal toxic, double-strike toxic, nonlethal infect, ordinary
lethal damage beside nonlethal infect, and zero-power toxic.

Verification: targeted game matrix, policy60, headless bot1, strict policy
all-target Clippy, formatting/diff pass. Full card suite: 1,499 pass / 15 ignored. Paired Demo arena (8 seeds,
64 playouts, horizon 1, two threads) unchanged: 68.8% ±17.9%, +137 Elo
[+6,+325], 16 games, no adjudications, mean 15.3 turns, 129.0s,
3096 searched decisions, 39 overrides.

Limits: printed keywords, potential unblocked combat damage rather than a full
block/damage assignment simulation. No attack-side poison win planning or
search core/eval changes. No general strength gain established by targeted tests.
