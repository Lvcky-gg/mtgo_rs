# Generic tap and untap triggers

Added generic singular battlefield noun trigger phrases “becomes tapped” and
“becomes untapped,” mapped to existing filtered BecomesTapped/BecomesUntapped
patterns. Existing EventSubject binding supplies that permanent/creature and its
controller. No engine or IR changes.

Frozen comparison: 15,864 → 15,872 / 34,913 (45.5%), eight gains, zero losses.
New supported cards: Gideon’s Avenger, Judge of Currents, Lifeblood, Lifetap,
Magda, Brazen Outlaw, Mesmeric Orb, Thoughtleech, Wake Thrasher.
Snapshot: /tmp/mtgo-general-tap-comparison; outputs:
/tmp/mtgo-general-tap-before.txt and /tmp/mtgo-general-tap-after.txt.

Two new gameplay tests in compiled/untap_triggers.rs verify Mesmeric Orb mills
both friendly and opposing untapped permanents’ controllers, and generic tap
triggers apply creature/controller filters and bind the event subject correctly.
Oracle suite: 841 passed, 15 existing ignored. Oracle all-target Clippy passes.
