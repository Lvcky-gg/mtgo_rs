# Minimizer regression inputs

These scenarios deliberately fail. They run through the independent
`minimization_review` tests, rather than the passing replay/regression corpus.

`checkpoint_substitution.json` preserves a deliberately incorrect final-action
digest. A reducer must retain that failing checkpoint, even when deleting earlier
actions would make an earlier checkpoint stale with the same generic message.

Reduction preserves observable assertions, not proof of equivalent Magic causes.
An independent rules review must approve a reduced reproduction before it enters
the engine regression corpus. Never record new expectations to bless a reduction.
