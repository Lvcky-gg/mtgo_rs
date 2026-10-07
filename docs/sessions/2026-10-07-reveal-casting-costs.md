# Reveal additional casting costs

The compiler accepts revealing a fixed number of matching cards from your hand
as an additional casting cost, including the reveal-or-pay-mana choices on
Thunderherd Migration and Goldmeadow Stalwart. Parsing is scoped to additional
casting costs, rather than exposing unimplemented reveal activation or kicker costs.

The engine offers matching cards from the caster's hand and excludes the spell
being cast. Chosen cards join a separate revealed payment list, rather than the
list used for sacrifice or discard. Payment logs Revealed events with CostPayment
causes and leaves the cards in hand. Reveal affordability feeds the existing
additional-cost-choice path, so unavailable choices are removed and the mana
alternative still includes the spell's base cost.

Four regressions cover reveal/mana choices with both or only one affordable,
no movement of revealed cards, mana actually paid, exclusion of the spell and
opponent's hand, revealing two blue cards before resolution, and full printed text.

Validation: 1,221 engine/Oracle tests passed, 38 ignored. Workspace all-target,
all-feature Clippy and formatting pass. Live audit: 16,428 / 34,913 cards playable
as printed (47.1%), up eight from the starting 16,420 snapshot.
