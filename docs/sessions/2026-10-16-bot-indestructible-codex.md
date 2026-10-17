# Indestructible combat assessment

The shared heuristic fight estimate previously treated indestructible creatures
as destroyed by lethal damage or deathtouch. Stats now records the printed keyword
and damage destruction checks respect it. Both attack and block decisions use this
estimate, including first strike and double strike.

Three policy regressions cover damage/keyword combinations, taking a safe block,
and avoiding a deathtouch attack that loses without killing its blocker. A compiled
card game additionally verifies that the bot chooses the block and the real engine
leaves the indestructible defender alive while the attacker dies.

Verification: policy58, headless bot1, compiled-card scenario1; strict policy
all-target Clippy and formatting/diff checks pass. Full compiled suite: 1,494 pass / 15 ignored. Paired Demo arena (8 seeds,
64 playouts, horizon 1, two threads) unchanged: 68.8% ± 17.9%, +137 Elo
[+6,+325], no adjudications, mean 15.3 turns, 135.0s, 3096 searched decisions,
39 overrides. This does not establish a general strength gain.

Scope: printed indestructible only. Conditional or externally granted keywords
still need richer public characteristics in the view. No search core/eval changes.
