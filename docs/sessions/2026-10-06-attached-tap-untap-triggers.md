# Attached creature tap and untap triggers

The enchanted/equipped creature event table now recognizes “becomes tapped”
and “becomes untapped,” using existing attachment-filtered event patterns and
EventSubject binding. No engine/IR changes.

Frozen comparison: 15,891 → 15,895 / 34,913 (45.5%), four gains, zero losses.
The baseline includes concurrent Claude changes since the previous batch.
New supported cards: Betrayal, Hawkeye's Bow, Insolence, Lust for War.
Snapshot: /tmp/mtgo-host-tap-comparison. Comparison outputs:
/tmp/mtgo-host-tap-before.txt and /tmp/mtgo-host-tap-after.txt.

Two gameplay regressions in compiled/untap_triggers.rs verify Insolence damages
only its host's controller, ignores unrelated taps and already-tapped hosts,
and equipped untap triggers follow the current attachment after changing hosts.
Oracle suite: 844 passed, 15 existing ignored. Oracle all-target Clippy passes.
