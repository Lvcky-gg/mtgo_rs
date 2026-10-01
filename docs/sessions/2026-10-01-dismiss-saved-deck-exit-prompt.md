# Dismiss stale unsaved-deck prompts

Saving a draft successfully now clears the pending discard confirmation. The
top bar also clears that confirmation whenever the draft is no longer dirty,
including when the name is restored to its saved value. Failed saves retain the
prompt and the draft, so the player can keep editing or explicitly discard.

Validation: all 31 existing builder tests and app library/binary Clippy with
dependency lints disabled and warnings denied pass. Source formatted directly;
diff checks pass. Changes remain uncommitted.
