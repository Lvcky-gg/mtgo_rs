# Respect focused text inputs for game shortcuts

The game screen now checks egui's keyboard-input demand before consuming undo
or pass shortcuts. Ctrl/Cmd+Z remains available to a focused numeric/text editor
instead of undoing a game action or selection. Space uses the same focus check.

GUI build and app/store all-target Clippy with dependency lints disabled and
warnings denied pass. Interactive visual inspection was not performed.
