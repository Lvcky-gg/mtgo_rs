# Keep lobby progress responsive

The waiting screen now retains the latest 100 status messages in a VecDeque and
shows them in a 240-point-high scroll area that follows new messages when already
at the bottom. Long runs of failed connection attempts no longer grow the displayed
history indefinitely or push current progress below the visible screen.

UI event draining now handles at most 128 queued events per frame and requests
an immediate repaint when that batch limit is reached. Remaining events retain
their queue order and are handled in following frames, allowing drawing and input
handling between large event batches. The worker-to-UI channel itself remains
unbounded.

Changes are confined to `mtg-app/src/app.rs`; concurrent rules/compiler work was
preserved. This is a small UI change; no new tests were added.

Validation: GUI binary build, app all-target Clippy with dependency lints disabled
and warnings denied, and `git diff --check` pass. The GUI was not launched for
visual inspection.
