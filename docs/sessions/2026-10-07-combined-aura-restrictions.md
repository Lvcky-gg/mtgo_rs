# Combined Aura restrictions

The static restriction parser accepts the combined sentence "doesn't untap during
its controller's untap step and its activated abilities can't be activated",
with or without a comma. It emits both existing restrictions. No engine or IR
changes were needed; the paid-mana parser and concurrent engine work were preserved.

Full printed rules text now compiles for Encrust, Stuck in Summoner's Sanctum,
and Hold for Questioning, verified by a separate compilation regression. A
parameterized gameplay regression checks ordinary and mana ability suppression,
untap suppression across turns, an unaffected second artifact, and restoration
after destroying the Aura. Its Aura cost uses only blue and the artifact produces
colorless mana so automatic payment cannot tap the control artifact.

The full Oracle run passes 83 unit, 761 compiled, 35 conversion, and 10 import
tests (889 passed; 15 existing ignored). The printed-rules regression was added
after that run and passes separately. Diff whitespace checks pass. Concurrent
changes continued during validation, so no isolated coverage gain is claimed.
The earlier depletion-land regression passes in this run.
