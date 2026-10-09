# Deliberately failing harness fixtures

These inputs verify failure reporting, capture, replay and minimization. They are
tested by independent Rust harness tests, outside the passing engine regression
and replay corpora.

`advance_budget_exhaustion.json` stops a legal automatic transition with a budget
of one. It reproduces the former inability to capture/minimize advance exhaustion.
It must produce structured FAIL evidence with no divergent action (no answer has
been applied), and that observation must reproduce through a replay attachment.
This bounded interruption does not establish an engine infinite loop.
