# Report land arrivals without assuming they were played

Battlefield-entry narration now uses "entered the battlefield" for lands as
well as other permanents. Projected views do not distinguish playing a land
from putting one onto the battlefield through an effect, so the previous
"played" message could falsely describe a land returned from a graveyard.

A regression covers a graveyard land returning to the battlefield. Existing
land-arrival and spell-cast coverage remains in place with factual wording.

Validation: all 18 narration tests, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass. Changes are uncommitted.
