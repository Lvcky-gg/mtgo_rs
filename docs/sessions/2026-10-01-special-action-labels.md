# Name special actions and show their immediate costs

The card-text snapshot now records special-action labels by engine ability ID for
foretell, plot, suspend, and turning a morph/disguise card face up. The UI uses
these labels for SpecialAction instead of the generic "Use" button. Labels show
the mana cost of the immediate action, not the later casting cost. Unknown or
hidden cards retain a generic label without exposing identity.

A regression verifies foretell versus plot, sparse ability IDs, turning face up,
immediate versus later costs, and hidden-card fallback. Code changes are in
cards_text.rs and ui.rs; engine/compiler work was preserved. No GUI visual
inspection was performed.

Validation: all eight card-text tests pass; app compilation, app all-target Clippy
with dependency lints disabled and warnings denied, and diff checks pass.
