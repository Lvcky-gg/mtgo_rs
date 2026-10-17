# Toxic combat defense

Blocking now includes printed toxic in the poison estimate. Unlike infect, toxic
also leaves ordinary life damage intact. Multiple printed toxic abilities add
up; infect and toxic combine; double strike can apply toxic twice. No toxic
poison is counted when no combat damage reaches the player.

A seven-case compiled-card game matrix verifies lethal and survivable poison,
ordinary life lethality, infect plus toxic, double strike, several instances of
toxic, and a zero-power attacker. Each checks the bot's chosen blocks and real
engine life/poison totals.

Verification: targeted scenario and policy59 pass, headless bot1, strict policy
all-target Clippy, formatting/diff checks pass. Full card suite: 1,498 pass / 15 ignored. Paired Demo arena (8 seeds,
64 playouts, horizon 1, two threads) unchanged: 68.8% ±17.9%, +137 Elo
[+6,+325], 16 games, no adjudications, mean 15.3 turns, 122.8s,
3096 searched decisions and 39 overrides. No general strength gain established.

Limits: printed keywords only. Existing greedy blocker assignment and approximate
trample/first-strike estimates remain, including conservative toxic hit counts
for double strike with trample. Attack-side poison wins are still future work.
No engine or search core/eval changes.
