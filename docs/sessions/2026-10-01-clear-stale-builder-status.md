# Clear outdated builder status

Changing a draft name now clears previous saved, copied, or error status, so the
message no longer describes an earlier name. Successfully generating a sample
hand also clears previous status, including a library-size error after the deck
has been corrected. Failed sample generation continues to display its error.

Validation: package formatting, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass. This is a small status-display
change; no new tests were added. Changes remain uncommitted.
