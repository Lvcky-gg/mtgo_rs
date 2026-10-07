# Batch tap and untap triggers

The “one or more” trigger branch accepts become tapped/untapped, using existing
OncePerBatch limits. The quantifier now supplies plurality, supporting invariant
subtype spellings such as Merfolk. No engine/IR changes. Added Merfolk to the
compiled test harness subtype list.

Frozen comparison: 15,954 → 15,955 / 34,913 (45.7%), one gain, zero losses:
Deeproot Pilgrimage. Baseline includes concurrent Claude changes.
Snapshot /tmp/mtgo-batch-tap-comparison; outputs /tmp/mtgo-batch-tap-before.txt
and /tmp/mtgo-batch-tap-after.txt.

Two tests in compiled/batch_tap.rs verify simultaneous Merfolk taps make one
token, token-only taps do not trigger, later taps can trigger again, and normal
untap-step simultaneous changes trigger once before upkeep.
Oracle suite: 853 passed, 15 existing ignored. All-target Oracle Clippy passed.
