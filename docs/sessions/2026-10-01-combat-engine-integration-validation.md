# Validate combat fixes across the engine

cargo test -p mtg-engine completed successfully across all enabled library and
integration tests, including combat, turns, undo, triggers, costs, and abilities.
Existing ignored ability-extension tests remain skipped; this validation does
not establish support for those incomplete features.

All 17 app UI tests pass with the default-damage preview and engine fixes.
App all-target Clippy with dependency lints disabled and warnings denied and
diff checks pass. Concurrent policy unused-import warnings remain unchanged.

Commands: cargo test -p mtg-engine;
cargo test -p mtg-app --lib ui::target_tests;
cargo clippy -p mtg-app --all-targets --no-deps -- -D warnings;
git diff --check. No commit created for this validation.
