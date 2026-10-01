---
created: 2026-09-30
type: session
tags: [coding/session, project/mtgo-rs, lang/rust, rules, ux]
related: ["[[mtgo-rs-architecture]]", "[[ADR-013-compile-rules-text-behind-a-strict-gate]]", "[[2026-09-30-paid-mana]]"]
---

# Planeswalker loyalty and combat

Continued Claude's rules backlog after the paid-mana session.

Implemented fixed positive, negative, and zero loyalty symbols as signed activation
costs. The engine enforces own-main-phase/empty-stack timing, sufficient loyalty,
and one loyalty activation per permanent per turn across its abilities. Costs are
paid before resolution; the ability resolves even when its source dies from zero
loyalty. Undo restores the counters and activation allowance. Fixed mana-producing
loyalty abilities use the stack rather than the immediate mana-ability path.

Attack declarations now include legal destinations: the defending player and their
planeswalkers. Attackers may choose different destinations. Legacy Objects answers
still attack the player; explicit Attackers answers pair creatures with destinations.
Duplicate, ineligible, and illegally directed attackers are rejected without mutating
combat. The defending seat is recorded in the attack event, so it remains known when
an attacked planeswalker leaves. Combat remains the existing two-seat flow.

Object damage records capture the recipient's relevant types. Spell and combat
damage share this construction and apply loyalty loss, creature damage, or both in
one event, preserving trigger attribution and lifelink counting. Departed attack
destinations receive no damage and generate no lifelink gain; damage is not redirected
to their controller. Activation history is also recorded through applied events.

The UI offers a destination for each selected attacker. Attack destinations cross
in PlayerView and appear on the board and in narration. Activated-ability buttons
now show their printed costs and effects, keyed by ability id, instead of identical
Activate-card labels for every ability on a permanent.

Validation:
- cargo test --workspace --all-features: 657 passed, including socket matches.
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean.
- 120 printed-text-to-game scenarios, including timing, payment, zero loyalty,
  targeted loyalty effects, undo, blocking, trample, lifelink, lethal damage,
  departed destinations, damage triggers, and creature/planeswalker damage.
- Attack declarations and projected destinations round-trip through JSON.
- The card database was read into a separate SQLite backup under /tmp for coverage:
  8,472 / 34,898 cards (24.3%), versus Claude's recorded 8,355 (23.9%). This increase
  includes the preceding paid-mana work. The live database was not updated.

Rules reference: Wizards' Comprehensive Rules, CR 606, 508, 120:
https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf

Still unsupported: variable loyalty costs; loyalty cost modifiers; choosing colours
for mana-producing loyalty effects; effects/emblems the grammar cannot express;
multiplayer combat and battle attack destinations. The compiler continues to reject
unsupported text. The simple bot still lacks a strategy for using loyalty abilities
and choosing planeswalker attacks. The new UI controls were compiled and their
supporting logic tested, but this session did not run a visual GUI exercise.

Next larger backlog items: multi-face casting, damage prevention, copy effects,
granted non-keyword abilities, and the previously recorded shuffle-seed information
leak. Automatic planning for paid mana sources is also still outstanding.

This log is in the repository because the Brain vault is outside the writable
workspace. No git commit was created; project sources were already untracked.
