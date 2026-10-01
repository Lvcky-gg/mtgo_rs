# Card choice previews and enlargement

Bottom object, discard, target, and legend choices now draw their cards alongside
selection controls. Selected cards have an outline; reaching the selection limit
still permits inspecting other candidates.

During ask-and-replay resolution, the engine keeps the choosing player's view
before rolling back. Session host, match, and headless trace adapters use this
question-aware projection. This fixes newly drawn discard candidates appearing
hidden for draw-then-discard spells such as Faithless Looting. Other players keep
their normal private-information projection; the snapshot is valid only for its
pending question ID.

Card hovers offer Enlarge card on the board, in hand, on the stack, in choices,
in the builder/search/sample, and during sideboarding. The shared popup persists
until closed and displays cached or asynchronously loaded art at a larger size.
Cards without available images retain the text fallback.

Validation:
- All 194 app tests pass, including new rendering/selection and popup regressions.
- All 23 session tests and the headless bot test pass.
- The draw-then-discard regression passes and checks opponent privacy.
- App/session/headless all-target Clippy with dependency lints disabled passes;
  dependency mtg-policy currently reports unused imports.
- Broader compiled-card suite: 419 passed, 15 ignored, 10 failed in trigger-related
  scenarios. Concurrent engine detection edits remain untouched. This broader
  suite is not green; no claim of a clean full-workspace run.
- Diff whitespace check passes. No manual desktop visual test or commit created.
