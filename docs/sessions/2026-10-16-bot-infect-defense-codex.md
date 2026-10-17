# Infect combat defense

The bot used life as the survival threshold for every attacker. It could ignore
lethal infect at high life, or sacrifice a blocker against survivable infect
because its life was low. Stats now records printed infect. Blocking estimates
separate poison damage from ordinary life damage and preserve player-first
priority: prevent poison death, then life death, then planeswalker damage.
Infect still damages planeswalkers normally for this estimate.

A four-case policy matrix covers lethal/nonlethal poison, low life with only
nonlethal infect incoming, and competition with ordinary lethal damage. A
three-case compiled-card game matrix verifies the actual block assignments and
postcombat life/poison totals, including survival at nine poison.

Verification: policy59, headless bot1, targeted compiled scenario pass; strict
policy all-target Clippy, formatting/diff pass. Full card suite: 1,497 pass / 15 ignored. Paired Demo arena (8 seeds,
64 playouts, horizon 1, two threads): unchanged 68.8% ±17.9%, +137 Elo
[+6,+325], 16 games, no adjudications, mean 15.3 turns, 107.3s, 3096
searched decisions, 39 overrides. No strength or speed gain established.

Limits: printed infect only, no toxic, and no attack-side poison win calculation.
The existing greedy safe-block phase and trample/first-strike estimates remain;
this is not a complete block assignment solver. Search core/eval unchanged.
