# Verify deathtouch custom damage validation

An end-to-end regression attacks with a 4-power deathtouch creature into three
6-toughness blockers and submits a non-default 1/2/1 allocation. It verifies the
answer differs from the trusted default, is accepted through custom validation,
and all three blockers die. This proves one damage to an earlier blocker meets
the deathtouch lethal threshold before later blockers receive damage.

Validation: all 28 combat tests, engine library/combat-test Clippy with dependency
lints disabled and warnings denied, and diff checks pass. No production change
was needed; this covers an unresolved edge case of the new validator.
Changes remain uncommitted.
