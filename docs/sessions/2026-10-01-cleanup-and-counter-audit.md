# Cleanup timing and opposing counter audit

Continued the gameplay audit, prioritizing timing rules that affect instants.

Cleanup previously queued discard triggers but ended the turn without granting
priority to resolve them. A regression reproduced this with a discard watcher and
a draw-two instant. The engine now records whether cleanup performed a state-based
action or has triggered abilities waiting, grants priority for that exception,
and begins another cleanup after an empty stack and consecutive passes (CR 514.3a).
The repeated step still logs its end and empties mana. Undo checkpoints preserve
the flag, and the repeat clears previous-step undo history.

Tests cover responding with an instant during cleanup, resolving the discard
triggers, discarding again after the draw, and resolving everything before the next
turn. A separate test covers an SBA without a trigger. Ordinary cleanup is checked
to have no priority and no extra cleanup pass.

Opposing +1/+1 and -1/-1 counters previously never cancelled. A regression reproduced
both types persisting into priority. The engine now removes matching counters as
an SBA (CR 704.5q), rather than during individual counter addition. Removals are
appended after zone changes in the simultaneous batch so a creature dying in the
same check retains its pre-SBA counters in last-known information (CR 704.8), as
required for abilities such as undying. Phased-out objects are excluded from the
new cancellation check.

Validation before the final workspace run: 816 engine/oracle tests passed, 38
ignored; 220 app/session/headless tests passed. Final rules suite passes all 20
tests. Scoped engine library/rules and oracle compiled-test Clippy with dependency
lints disabled and warnings denied passes. Dependency mtg-policy retains unused
import warnings. No commit or manual desktop game created.

Rule references:
https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt
https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf
https://media.wizards.com/2025/downloads/MagicCompRules%2020251114.pdf

Final full workspace run: 1,193 passed, zero failed, 38 ignored. Whitespace diff
check passes. Changes remain uncommitted.
