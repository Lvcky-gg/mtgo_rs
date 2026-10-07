# Explicit printed flavor labels

Added explicit strip_ability_word entries Beacon of Hope, Keen Senses, Blood
Chalice, Rapacious Hunger, Martyrdom, Poison Breath, Medicus Ministorum.
Unknown prefixes and unsupported underlying clauses still fail compilation.
No engine/IR changes.

Frozen comparison: 16,008 → 16,012 / 34,913 (45.9%), four gains, zero lost:
Devoted Paladin, Owlbear, Sanguinary Priest, Sister Hospitaller.
Snapshot /tmp/mtgo-flavor-labels-comparison; outputs
/tmp/mtgo-flavor-labels-before.txt and /tmp/mtgo-flavor-labels-after.txt.

Four gameplay tests in labeled_triggers.rs cover entry draw, source death life
and draw, another creature dying and a source counter, and returning a five-MV
creature from the graveyard with five life gained after its object identity changes.
Oracle tests and all-target Clippy pass; 15 existing ignored tests.
