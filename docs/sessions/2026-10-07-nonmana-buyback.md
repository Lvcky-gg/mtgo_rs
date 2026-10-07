# Nonmana buyback

The compiler accepts printed Buyback— costs through the existing chosen-cost
kicker parser, with buyback enabled only on instants and sorceries. Sacrifice,
discard, and chosen tap costs use the existing engine payment path. No engine or
IR changes were needed.

A parameterized gameplay regression covers sacrifice-a-land and discard-two-card
costs, paying and declining each. It checks damage resolution, the spell's return
to hand, and whether the cost cards actually leave their original zones. Both
buyback tests pass, including the existing mana buyback regression. CLI compilation
also verifies the sacrifice cost is marked buyback.

The broader Oracle run passed 83 unit tests and 756 compiled tests, with 15
existing ignored. One compiled test failed:
mana_followups::depletion_land_is_sacrificed_when_its_last_counter_is_removed,
whose mana activation is refused as NotLegal. This test uses no buyback; concurrent
compiler/condition work appeared during this session and was preserved. No isolated
coverage gain is claimed: live coverage changed during the audit.

Rerunning the compiled suite with that one test excluded passes all 756 remaining
tests. The depletion test changed concurrently; a later focused run instead
failed its final sacrifice assertion. Diff whitespace checks pass.
