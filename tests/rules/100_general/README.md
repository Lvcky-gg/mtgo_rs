Independent primitive golden propositions execute in
`crates/mtg-verify/tests/core_actions.rs`, grounded in the official Wizards
Comprehensive Rules: https://magic.wizards.com/en/rules.

- CR 121.1/121.2: individual draws move successive library-top cards into the
  drawing player's hand and emit individual draw events.
- CR 121.4/704: failed draws cause loss at the following SBA check; later
  instructions of the resolving effect still happen.
- CR 121.5: moving library cards into hand without drawing emits no draw event
  and does not cause an empty-library draw loss.
- CR 701.8: the affected player selects a discard into the owner's graveyard.
- CR 701.13: milling uses library-top cards, performs as much as possible, and
  does not count as drawing or cause empty-library draw loss.
- CR 119.3/119.9: life gain/loss changes the proper total; zero gain is no event.
- CR 120.3/120.8: ordinary player damage loses life, creature damage marks damage
  without reducing toughness, and zero damage is no event.
- CR 400.7: a zone change creates a new object identity while preserving card
  and owner; battlefield damage/tapped status does not survive exile.

These tests drive production effect resolution directly with invented card IR,
checking independently specified state and event semantics. They do not certify
casting, all replacements, or all keyword variants of the tested effects.
