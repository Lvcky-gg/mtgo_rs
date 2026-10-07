# Return-land alternative costs

The compiler understands alternative costs that return a specified number of
permanents you control to their owners' hands, including one/two/three Islands
and paying mana plus returning a basic land. Return-cost parsing is scoped to
plain alternative costs; other activated or kicker return costs remain unsupported.

AdditionalCost::ReturnToHand carries the filter and count. The engine offers only
matching permanents controlled by the payer, asks during casting, excludes these
objects from sacrifice/discard payment, and returns them through the existing
owner-relative return payment path. Mana is paid before the selected objects are
returned, so the returned land can supply required mana. Countering the spell
does not refund the returned lands. The IR walker visits the filter and count.

Four gameplay regressions cover one/two/three-land payments, immediate payment
before resolution, countering, mana from the returned land, differing owner and
controller, unavailable alternative payments, and normal casting without returns.

Validation: 1202 engine/IR/Oracle tests passed, 38 ignored; workspace
all-target/all-feature Clippy and formatting check pass. Database coverage:
16,351 / 34,913 playable as printed (46.8%), compared with 16,341 previously.
