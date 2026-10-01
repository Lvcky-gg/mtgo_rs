# Verify question controls with real UI clicks

Added headless egui tests that render questions and send pointer clicks to their
controls, checking answers on the actual UI channel. Coverage includes:

- Incomplete multi-slot targets cannot be confirmed; complete selections wait for
  confirmation and preserve slot order.
- Single-target choices still submit immediately.
- Optional single modes allow choosing none or selecting one with confirmation.
- Mandatory single modes still submit immediately.
- Large X values can be confirmed, and small ranges at u32::MAX do not overflow.
- Blocker movement submits the displayed order rather than trigger placement order.

The harness uses egui 0.36's run_ui API and explicitly clears renderer texture
deltas because it has no GPU renderer. It creates no image jobs or network requests.
These interaction checks supplement existing pure selection/backlog tests.

All ten UI tests, app Clippy with dependency lints disabled and warnings denied,
and diff checks pass. Integrated app checks are recorded in the batch handoff.
