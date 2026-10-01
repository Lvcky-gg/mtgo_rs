# Include defending players in attack narration

Attack messages now include announced defending players as well as permanent
targets. The viewer is named "you" in this position. Unavailable permanent
targets use the existing hidden-name fallback, and missing target information
retains the previous attacker-only wording.

A regression covers both player names, a visible permanent, a missing permanent,
and repeated views remaining quiet.

Validation: all 22 narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Changes are uncommitted.
