# Release pipeline repair — 2026-10-17

The latest main commit's Release and Verification workflows failed at strict
Clippy checks. Release version allocation, both macOS builds, the Windows
build, and Flatpak packaging succeeded, but the failed test gate prevented
publication. Inspected runs:

- https://github.com/Lvcky-gg/mtgo_rs/actions/runs/38984918472
- https://github.com/Lvcky-gg/mtgo_rs/actions/runs/38984918385

Fixed the code flagged by the existing gate: nested entry-tapped checks,
boolean assertion, descending sort, hex digit grouping, integer divisibility
checks, an arena pairing type, and the evaluation solver's indexed inner loop.
The solver retains the same pivot and arithmetic order by copying the unchanged
pivot row before iterating over the other rows. No workflow checks were
removed, disabled, or given lint allowances.

The latest push also includes the accumulated deck/legendary coverage work,
the existing arena per-game timeout reporting, and the search playout step
bound already present in the working tree when publication was requested.

Local validation:

- `python3 -m unittest discover -s scripts/tests -v`: 81 tests, one existing skip.
- `cargo fmt --all --check` and `git diff --check`: pass.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: pass.
- `cargo test --workspace --all-features --locked --quiet`: pass.
- `cargo build -p mtg-app --bin mtg-gui --release --locked`: pass.
- PR verification campaign: all 35 planned checks passed, including property,
  replay, regression-corpus, and bounded semantic checks.
- Standard verification policy gate passed. Closed-alpha assessment was not
  requested; the release workflow retains its existing policy.
