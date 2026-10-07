# Generic damage triggers

Generic singular battlefield noun trigger parsing now accepts “is dealt damage”
and “is dealt combat damage.” Uses existing TakesDamage with its combat-only
flag, EventSubject and EventAmount bindings. No engine/IR changes.

Frozen comparison: 15,902 → 15,908 / 34,913 (45.6%), six cards gained, seven
faces gained, zero lost. New cards: Death Pits of Rath, Ill-Tempered Loner
(including Howlpack Avenger), Kazarov, Sengir Pureblood, Repercussion,
Rite of Passage, Termination Facilitator.
Snapshot /tmp/mtgo-generic-damage-comparison; before/after outputs are
/tmp/mtgo-generic-damage-before.txt and /tmp/mtgo-generic-damage-after.txt.

Two parameterized gameplay tests in compiled/generic_damage_triggers.rs cover
Repercussion damage amounts and both controllers, combat-only triggers ignoring
spell damage, Rite of Passage controller filtering, and lethal damage occurring
before the counter trigger can resolve.
Oracle suite: 848 passed, 15 existing ignored. All-target Oracle Clippy passed.
