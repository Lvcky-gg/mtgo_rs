# Report changed blocking assignments

Blocking narration now compares the previous attacker ID with the current one,
rather than suppressing every assignment on a creature already blocking.
Changed assignments are reported, while unchanged assignments remain quiet.

A regression covers a changed assignment, repeated identical views, and an
unavailable attacker using the existing generic name fallback.

Validation: all 21 narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Changes are uncommitted.
