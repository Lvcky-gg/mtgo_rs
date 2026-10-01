# Retain incomplete deck imports for correction

Successful partial imports no longer clear the pasted deck list. Input is cleared
only when every card line was parsed and matched. The summary explains that the
list remains available for correction; saving matched cards retains the existing
partial-import behavior.

ImportSummary::is_complete centralizes this decision. The existing import
regression now checks both an unmatched card and an unreadable count, along with
a fully matched import. Import failures continue preserving input as before.

Validation: all five deck tests, GUI build, app all-target Clippy with dependency
lints disabled and warnings denied, and diff checks pass. Changes remain
uncommitted.
