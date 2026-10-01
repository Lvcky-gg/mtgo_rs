# Edit blocker combat-damage allocations

Combat prompts now offer Customize damage. The editor starts with the engine's
default amounts for each offered blocker, provides bounded numeric controls,
and shows the assigned total. Confirm damage is disabled above the available
total. The prompt explains lethal ordering and when unused damage may trample
through. The engine validates the final allocation and re-asks rejected answers.

Use default instead and Undo cancel the editor without submitting an answer.
Custom state clears when the question is answered or replaced.

A headless regression checks default seeding, cancellation, Undo, over-allocation
rejection in the UI, and confirmation of an edited allocation. Existing engine
combat regressions cover legality and custom trample handling.

Validation: all 18 UI tests, app all-target Clippy with dependency lints disabled
and warnings denied, package formatting, and diff checks pass. Changes uncommitted.
