# Copy limits use English card names

Updated the shared deck validator to aggregate Constructed copies across the
main deck and sideboard by English card name, and Commander singleton checks by
English card name in the main deck. Commander duplicate checks now also catch a
card with the same English name as its commander even when its database identity
differs. Color identity remains checked for each card identity.

This follows Comprehensive Rules 100.2a, 100.4a, and 903.5b:
https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf

Validation: `cargo check --workspace`, Clippy for `mtg-session` and `mtg-app`,
and `git diff --check` passed. No tests were run.
