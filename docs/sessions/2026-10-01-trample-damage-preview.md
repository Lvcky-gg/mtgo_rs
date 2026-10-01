# Show excess trample damage in the default preview

Combat damage preview now includes default damage not represented by object rows
as excess trample damage to the publicly projected attack destination. If that
destination is unavailable it uses the generic Defender label. Totals use u64
arithmetic and saturating subtraction to avoid overflow from malformed rows.
The wire answer remains unchanged; the engine recovers the full canonical default.

A headless regression projects an attacker aimed at the opponent, assigns one
damage to each of two blockers out of four total, and verifies Opponent: 2 trample
damage is rendered before any answer is sent.

Validation: all 17 UI tests, app all-target Clippy with dependency lints disabled
and warnings denied, formatting, and diff checks pass. Changes uncommitted.
