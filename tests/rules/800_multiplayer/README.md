Independent Commander propositions execute in
`crates/mtg-verify/tests/commander.rs`:

- CR 400.1 / 903.8: a player can cast their own commander from shared Command;
  an opponent cannot cast it merely because it is publicly visible there.
- CR 903.8: each previous command-zone cast adds two generic mana to the cost.
  The generated boundary matrix covers zero through three previous casts.
- A taken commander-return option preserves shared Command zone representation
  and card ownership.

Primary source: https://magic.wizards.com/en/rules and Wizards Comprehensive
Rules sections 400.1, 903.8, 903.9a/903.9b.

CR 903.9a optional-return tests additionally cover owner acceptance/decline from
graveyard and exile, former-controller exclusion, no repeated prompt after
declining, fresh eligibility after zone reentry, mandatory deaths before
priority, APNAP decision order, simultaneous accepted moves, and preservation
of the outstanding choice after a malformed answer.

These initial propositions do not certify complete Commander support. The
hand/library replacement option under CR 903.9b, meld/merged commanders, deck
construction, commander damage, and wider interaction matrices still need
independent verification.

CR 800.4a/800.4d propositions execute in `crates/mtg-verify/tests/leave_game.rs`
(three-player games): owned objects in every zone leave the game; control
effects giving the departed player control end without a zone change; objects
still under their control are exiled; their abilities on the stack cease to exist
while a spell they control but don't own is exiled; conceding with priority does
the same and passes priority on; and a dies trigger they would control is not put
on the stack (with a control case where it is). Later additions: another
player's control effect persists; a remaining player's ability outlives its
source's departing owner; "each player" creates no token for a departed player;
a departed player is no one's opponent; and an attacker whose defending player
left assigns no combat damage (800.4e, lifelink control case). These were authored by the
Builder from the CR text, not by an independent agent. Primary source: Wizards
Comprehensive Rules, 2026-09-25 release, rule 800.4.
