# Show Aura and Equipment attachment targets

Cards with ObjectView.attached_to now show "Attached to …" alongside damage and
counter notes. Both card-image and fallback-panel rendering receive the note.
Target names come from the existing visible-object lookup, preserving hidden-card
fallback. This uses existing view data and does not change engine/view schemas.
Player-attached Auras cannot be labeled yet because attached_player is not in
ObjectView.

Code edits are confined to ui.rs; engine/compiler work was preserved. This is a
small display change; no new tests or GUI visual inspection were performed.

Validation: app all-target Clippy with dependency lints disabled and warnings
denied, and diff checks pass.
