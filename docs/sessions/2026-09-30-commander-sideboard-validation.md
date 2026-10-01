# Commander sideboard validation

The shared deck validator now reports a Commander deck with any sideboard cards
as invalid. Commander game setup draws only from the 99-card main deck, so those
cards would otherwise remain in the saved deck but disappear from play. The rule
uses the existing format distinction that Commander has no sideboarding.

Validation: `cargo check -p mtg-session -p mtg-app`, Clippy for both crates,
the GUI build, and `git diff --check` passed. No tests were run.
