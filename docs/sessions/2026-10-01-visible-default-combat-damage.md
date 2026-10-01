# Show default combat damage before confirmation

Combat damage prompts now show the attacker, its total damage, and the individual
object assignments supplied by the engine's default. Confirm default damage
submits that same answer. This replaces the generic fallback prompt without
changing assignment rules or automatically answering the question.

The headless regression checks both displayed blocker amounts, waits for
confirmation, and verifies the exact default rows are submitted.

Remaining engine/UI work: custom allocation requires engine-side legality
validation. The current Answer::DamageAssignment handler accepts rows directly;
its default also omits player targets when converting canonical trample damage
to object-only rows. No custom editor was added on top of that behavior.

Validation: all 16 UI tests, package formatting, app all-target Clippy with
dependency lints disabled and warnings denied, and diff checks pass.
Changes remain uncommitted; concurrent engine work was preserved.
