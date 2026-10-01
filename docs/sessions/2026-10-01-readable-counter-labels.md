# Readable counter labels

Known counters now display names such as "+1/+1", "loyalty", and "stun" rather
than Rust Debug variant names. Card state notes and permanent summaries use totals
such as "2 +1/+1 counters" or "1 stun counter". Trigger-order overlap explanations
also use the readable names.

Custom counter names are not carried in the view, so Other counters use "other"
as a fallback. No engine/view schema changes were made. Code edits are in
mtg-app format.rs and ui.rs; concurrent engine/compiler work was preserved.
No GUI visual inspection was performed.

Validation: all 12 formatting tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass.
