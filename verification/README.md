# Rules primitive confidence

`primitives.json` is a versioned registry, initially containing **no accepted
evidence**. Every primitive and synthetic fixture dependency therefore evaluates
to EXPERIMENTAL. Synthetic fixture names do not claim complete dependency graphs
for database cards. This initial inventory is deliberately incomplete.

`ConfidenceRegistry::load` validates bounded JSON; `report()` evaluates primitive
and card confidence for the registry's assessed build. Missing dependencies are
reported and propagate EXPERIMENTAL status. An empty dependency graph is also
EXPERIMENTAL. Consumers must compare the assessed build identity with the engine
build used for their simulations; stale evidence never transfers automatically.

Evidence records reference artifacts and their authors/reviewers. Those strings
are **advisory provenance, not authenticated review guarantees**. A trusted CI and
human review process must approve artifact provenance and ensure the reported
counts correspond to actual campaigns. Editing the JSON can fabricate metadata;
the registry is not a cryptographic trust system.

VERIFIED requires same-build, independently reviewed golden, property,
differential, fuzz, and production mutation evidence meeting explicit thresholds.
Builder-only evidence may reach TESTED, never VERIFIED. Failing current-build
reports force EXPERIMENTAL. Controlled predicate mutation examples are excluded
from production mutation counts. A small curated mutation smoke result alone does not satisfy
the default minimum sample of 20 production mutations.
