# Cards from hand to library

The compiler accepts choosing a specified number of cards from your hand and
putting them on top or bottom of your library, optionally “in any order”. It also
accepts any-number placement followed by drawing that many cards, optionally plus
a fixed number. The existing ChosenBy and MoveZone operations provide private hand
selection, ordering, and new card identities; the subsequent count reads the moved
cards from Binding::It. Selecting none explicitly resets the count to zero.

Optional placement followed by “If you do, draw a card” uses the existing may
condition with a hand-availability check, including an ownership filter so an
opponent's cards cannot enable the action. Without enough cards, mandatory
placement does as much as possible; optional conditional placement requires the
full number before offering the choice.

Four regressions cover Brainstorm's draw-before-placement and top order, bottom
ordering and zero/one/two/three-card counts plus one, optional decline and empty
hand behavior, mandatory partial placement, and excluding opponents' hand cards.

Validation: 1217 engine/Oracle tests passed, 38 ignored. Workspace
all-target/all-feature Clippy and formatting checks pass. The live database audit
reports 16,420 / 34,913 playable as printed (47.0%), compared with the earlier
16,393 report. Concurrent compiler work contributes to this snapshot.
