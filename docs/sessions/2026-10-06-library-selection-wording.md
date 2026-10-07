# Library-selection wording — 2026-10-06

Extended clauses::dig to accept numeric “one/two of those cards” selections,
optional numeric selections, and comma-then introductions after the top-card look.
The remainder may now go into the controller's hand, using the existing resolver.
No IR or engine changes were needed.

Frozen-source comparison: 15,675 → 15,684 / 34,913 (44.9%), +9, zero lost.
Added Court Hussar, Genesis Ultimatum, Maestros Charm, Nissa, Nature's Artisan,
Orazca Puzzle-Door, Organ Hoarder, Prophetic Bolt, Scattered Thoughts, Stress Dream.
Claude's concurrent changes were held constant in the comparison.

Four gameplay regressions verify the mandatory numeric quota across sentence
and comma-then introductions, optional selection declining all cards, clamping
the requested quota to a short library, and Genesis Ultimatum's chosen permanents,
hand remainder, and self-exile. The latter test uses the shared exile zone.

Validation: Oracle suite 807 passed, 15 existing ignored; Clippy with
warnings denied and diff check pass. Source changes confined to clauses::dig,
new compiled/dig_wording.rs, module registration, README and coordination docs.
