# Narrate attachment changes

Persistent battlefield objects now report becoming attached, moving to another
object, and becoming unattached. Target names come from the current projected
view, with a hidden fallback when the target is unavailable. Unchanged
attachments produce no messages.

A regression covers attachment, detachment, unchanged views, and movement to
an unavailable target. Newly entering objects retain existing entry narration.
Player attachments are not exposed by ObjectView and remain outside this change.

Validation: all 17 narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Changes are uncommitted.
