# Show announced targets on stack cards

Stack card state notes now include the announced targets from ObjectView.targets.
Both image and fallback text-card displays show the note. Object targets use the
existing visible name lookup, and player targets use the existing player labels.
Repeated targets remain in their announced order rather than being deduplicated.
No target note is added to non-stack objects.

Changes are confined to ui.rs and use existing public view data. Engine/compiler
work was preserved. This is a small display change; no new tests or GUI visual
inspection were performed.

Validation: app all-target Clippy with dependency lints disabled and warnings
denied, and diff checks pass.
