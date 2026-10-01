# Report deck-screen database failures

The deck screen now retains and displays database-open errors instead of silently
retrying creation every frame. An explicit Retry opening database button schedules
another attempt. New deck is disabled while no active Store is available.

A successful open refreshes displayed data from the new active Store. Successful
refreshes also clear any previous open error, including recovery through a database
update. Ordinary creation and editing remain available with an empty but writable
database.

Validation: GUI build, app all-target Clippy with dependency lints disabled and
warnings denied, and diff checks pass. Interactive visual inspection was not
performed. Changes remain uncommitted.
