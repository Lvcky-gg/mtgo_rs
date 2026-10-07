The first independent golden propositions execute in
`crates/mtg-verify/tests/rules.rs`:

- CR 704.5f: nonpositive toughness moves even an indestructible creature to its
  owner's graveyard, with a fresh zone-change identity and SBA event cause.
- CR 704.5g and 702.12: lethal damage alone does not destroy an indestructible
  creature with positive toughness.

Source: Wizards of the Coast Comprehensive Rules, linked from
https://magic.wizards.com/en/rules (704.5f, 704.5g, 702.12).

These tests establish TESTED evidence only. The engine event vocabulary does not
have a separate Destroyed event, so the negative destruction assertion uses the
observable contrast with indestructible and the absence of regeneration. More
primitive golden coverage and independent review remain required.

CR 117.3d and 800.4 also have an independent multiplayer golden proposition:
two consecutive eliminated seats never receive priority, state and public
choice holders agree, and turn progression reaches the next surviving player.
