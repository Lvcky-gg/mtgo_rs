# Split tutors and Omen resolution

Library searches now support splitting the searched cards between the
battlefield and hand, as on Cultivate and Kodama’s Reach. A resolution-local
`ObjectFilter::InBinding` restricts the second choice to the original search
selection. Named bindings preserve that selection across zone changes, while
the existing resolver enforces mandatory selections, filters invalid answers,
and assigns new object identities when cards move.

The compiler also supports separately qualified alternatives such as “basic
land cards and/or Gate cards,” and a search/reveal instruction followed by a
separate sentence containing the split. Restrictions remain scoped to each
alternative: a nonbasic Gate qualifies, while an ordinary nonbasic land does not.

## Omen correction

Claim Territory exposed a pre-existing Omen bug: the shared Adventure layout
sent a resolving Omen into exile and granted permission to cast the creature.
The engine now distinguishes the printed Omen subtype. Successfully resolving
Omens move into their owner’s library and shuffle it; countered or fizzled Omens
go to the graveyard. Casting the creature half still enters the battlefield.
Copies shuffle as well and cease to exist under the existing state-based rules.
Other resolution replacement interactions retain the engine’s existing ordering.

Rules checked against the official release notes:
https://magic.wizards.com/en/news/feature/tarkir-dragonstorm-release-notes

## Coverage and validation

The same isolated 34,913-card database snapshot used for the previous batch:
**15,327 → 15,338 playable cards (43.9%), +11, no lost cards**.
No user database changes or card reimport are needed.

Eleven new regression tests cover split destinations and reveal order, either
land chosen for the battlefield, zero/one-card searches, malformed duplicate and
unrelated selections, a sacrificed tutor source, separately qualified Gate/Desert
alternatives, multi-sentence search instructions, resolving/countered/fizzled
Omens, the creature half, and copied Omens.

Validation passed:

- `cargo test -p mtg-oracle -p mtg-engine -p mtg-ir -p mtg-policy --offline --quiet`
- `cargo clippy -p mtg-oracle -p mtg-engine -p mtg-ir --all-targets --offline -- -D warnings`
- `cargo build -p mtg-app --bin mtg-gui --offline --quiet`

Test totals: 1047 passed; 38 existing tests ignored.

## Newly accepted card faces

These twelve face names represent eleven cards; Bloomvine Regent and Claim
Territory are the two faces of the same card.

- Bloomvine Regent
- Circuitous Route
- Claim Territory
- Cultivate
- Flare of Cultivation
- Flourishing Bloom-Kin
- Kodama's Reach
- Map the Frontier
- Navigation Orb
- Peregrination
- Spoils of Victory
- Troop of Ponies
