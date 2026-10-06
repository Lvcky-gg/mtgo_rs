# Printing loader follow-up — 2026-10-05

The printing picker now reports a disconnected worker as an error, exposing the existing Retry button instead of leaving a permanent loading spinner. Successful results survive normal channel closure.

Printing pagination tracks visited URLs and rejects repeated pages before making another request. HTTP fetching is separated from page traversal so regression tests can exercise pagination without live Scryfall requests.

Five new tests cover pending/disconnected workers, successful channel closure, repeated pagination, missing/external next-page URLs, and collecting matching printings across multiple pages.

Validation: app suite passed (193 unit tests, 2 network integration tests, 15 UI integration tests; 3 ignored) before the final two pagination tests were added. Final targeted printing tests and Clippy were also run. Existing concurrent repository edits were preserved. Changes remain uncommitted.
