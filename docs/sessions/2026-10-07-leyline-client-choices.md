# Leyline client choices

The bot recognizes the opening battlefield selection prompt and chooses all
offered cards within the maximum, instead of treating them as optional discards.
Other hand-selection heuristics remain intact. The existing UI already renders
the prompt and supports optional object selection; no behavior change was needed.

Regression coverage checks the bot selects both opening cards and still declines
an optional discard, and verifies UI clicks can confirm none, one, or both cards
despite the engine default selecting all. All 41 policy tests and the focused UI
test pass. Workspace formatting has pre-existing differences in other crates;
only the two touched client files were formatted. Full app library validation
passes 230 tests with five existing ignored after rerunning outside the sandbox
to permit loopback sockets. Diff whitespace checks pass.
