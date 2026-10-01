# Shared Constructed copy limit

Moved the Constructed four-copy check into `mtg-session::game::deck_problems`.
It now counts each non-basic card across main deck and sideboard, so deck-builder
feedback and game-start validation use the same rule. Removed the builder's
previous duplicate check.

Validation: `cargo check -p mtg-session -p mtg-app`, Clippy for both crates,
the `mtg-gui` build, and `git diff --check` passed. No tests were run.
