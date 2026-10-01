# Import into the active database connection

The deck import button now uses the screen's existing Store instead of creating
a separate default-path connection. After importing, the app refreshes card
counts, deck choices, and coverage from that same Store. The regular refresh
method retains its existing reopen behavior for other flows.

This keeps resolution, saving, and displayed results tied to one active database
and avoids an extra connection during import. Complete and partial input-retention
behavior is preserved.

Validation: all five deck tests, GUI build, app all-target Clippy with dependency
lints disabled and warnings denied, and diff checks pass. Changes are uncommitted.
