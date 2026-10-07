# Plural library placement

The compiler accepts plural targeted permanents placed into their owners'
libraries and plural targeted cards from your graveyard placed into your library.
This covers shapes printed on Footbottom Feast, Forever Young, Gravepurge,
Frantic Salvage, Plow Under, and Rebuking Ceremony. Indexed placement remains
singular; sending another owner's graveyard cards into your library is rejected.

MoveZone resolution groups top/bottom placements by owner and asks each owner
for an order before moving cards. Top insertions run in reverse so the first
chosen card is topmost; bottom insertions retain the chosen order. This uses
existing choices and IR. Concurrent mana/compiler work was preserved.

Five tests cover zero/one/several optional graveyard targets and the follow-up
draw, an opposing owner's ordering choice, different owners' libraries, bottom
ordering below existing cards, and unsupported destination rejection. The full
engine/Oracle run passed 1,163 tests with 38 existing ignored before the last two
tests were added; all five focused regressions pass afterward. Engine/Oracle
all-target Clippy with warnings denied and diff whitespace checks pass.

Files: clauses::put_into_library, the MoveZone resolver's ordering step,
compiled/plural_library.rs and registration, README, coordination notes.
Changes remain uncommitted. No isolated coverage gain is claimed while other
compiler work is active.
