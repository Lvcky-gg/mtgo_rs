# Life buyback and kicker

The compiler and kicker-cost filter accept fixed nonnegative PayLife costs.
spell_extra_cost now includes paid kicker parts, so the existing spell-payment
path reserves and pays life exactly once. kicker_affordable checks non-mana
payability, reserves combined additional life costs, and plans Phyrexian mana
against the remaining life. Mana-only kicker keeps its existing planning path.

Six regressions cover paid/declined/unaffordable buyback, exactly one life payment,
combined casting life costs, Phyrexian mana reservation, life kicker and entering
with counters, and paying all remaining life (legal, then losing before spell
resolution). Slaughter's complete printed text compiles. No IR changes.

Full engine/Oracle validation: 1,187 passed, 38 existing ignored. All-target
Clippy with warnings denied and diff whitespace checks pass. Changes are confined
to the compiler's chosen kicker gate, engine cost collection/affordability,
life_buyback.rs and registration, and handoff notes. Changes remain uncommitted.
