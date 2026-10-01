---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, ai, rules]
related: ["[[2026-09-30-planeswalkers]]", "[[mtgo-rs-architecture]]"]
---

# Bot plays with and against planeswalkers

Continued the planeswalker work by closing the bot's missing loyalty and combat
strategy. The bot still consumes only projected views and engine-offered choices.

The bot snapshots printed loyalty costs and simple effect targeting preferences.
It plays lands first, then a legal loyalty activation, then its most expensive
spell. It prefers the largest loyalty gain, falling back to zero or minus abilities
when those are the only ones offered. The engine enforces payment and shared limits.

A projected stack object now carries its public ability id, allowing the bot to
identify which effect is choosing targets. This field is optional for compatibility
and only populated for entitled stack objects. Direct damage, destruction, life
loss, and discard favour opposing targets; drawing and life gain favour friendly
targets. Mixed and unfamiliar effects use the existing default. Target selection
preserves the engine default's required count and picks only offered targets.

Combat takes estimated lethal damage to the player first. Otherwise the bot attacks
planeswalkers from lowest loyalty upward, sending spare attackers to the player.
Assignments use only offered destinations. Required attackers from the engine's
default are preserved even when the ordinary attack heuristic would hold them back.

Blocking measures incoming damage separately for the player and each planeswalker.
Safe kills come first, then chumps to save the player, then threatened planeswalkers.
Unblockable attackers remain in the damage estimate; menace is blocked with two
creatures when survival requires it. Trample estimates subtract the damage absorbed
rather than treating every chump as stopping the attacker. Visible counters and
marked damage adjust printed combat estimates.

Validation:
- cargo test --workspace --all-features: 672 passed, including socket matches and
  the existing bot-versus-bot game.
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean.
- Eleven new policy regression tests cover loyalty preference, legal destinations,
  split attacks, lethal player attacks, required attackers, planeswalker defence,
  survival priority, unblockable threats, menace, trample, and target counts.
- Four new imported-text-to-engine scenarios exercise loyalty activation once,
  planeswalker attacks, harmful targets, helpful targets, and planeswalker blocking.
  Every bot answer is accepted directly by the engine; there is no illegal-answer
  recovery hiding failures in those scenarios.

Limits: this is a greedy bot. It does not score ability effects strategically,
understand all continuous stat changes or combat keywords, plan paid mana chains,
or play instant-speed tricks. Those limitations affect strength, not the engine's
rules. The rules compiler did not change; the last measured card coverage remains
8,472 / 34,898 (24.3%).

Next major work remains multi-face casting, prevention, copy effects, granted
non-keyword abilities, and the documented shuffle-seed information leak.

The log is in the repository because the Brain vault is outside the writable
workspace. Project sources remain untracked; no git commit was created.
