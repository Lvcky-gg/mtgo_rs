# Card playability: type lists and dynamic quality bounds

The rules-text compiler now accepts explicit comma-separated type/subtype lists
of three or more alternatives, including Farseek’s four basic land types.
It also uses the existing amount parser for power, toughness, and mana-value
qualities. X is accepted only when the spell or activated ability defines X in
its cost. Lower, upper, and exact bounds reuse the existing engine filters;
toughness lower bounds evaluate X - 1, including when X is zero.

The parser preserves commas between separate effects. A coverage comparison
caught an initial regression in Lord of Tresserhorn; this was fixed and its
life-loss, sacrifice, and draw sequence is covered by an engine regression test.

## Measured coverage

Using an isolated snapshot of the local 34,913-card Oracle database:
**15,274 → 15,327 fully playable cards (43.9%), +53, zero lost cards**.
The original database was not modified. Compilation coverage indicates every
printed clause is recognized; the full list below is not an exhaustive gameplay
audit of every individual card.

## Validation

Seven new end-to-end/compiler regression tests cover Farseek’s four subtypes,
nonbasic dual lands and tapped entry; Chord of Calling’s X=0 and boundary filters;
Green Sun’s Zenith’s green restriction and return to the library after failed
searches; Citanul Flute’s activated X, reveal, hand destination and tap cost;
power/toughness/mana-value bounds; undefined X rejection; and effect-clause commas.

Commands:

- `cargo test -p mtg-oracle -p mtg-engine -p mtg-policy --offline --quiet`
- `cargo clippy -p mtg-oracle --all-targets --offline -- -D warnings`
- `cargo build -p mtg-app --bin mtg-gui --offline --quiet`

The existing ignored tests remain ignored. No database reimport is required;
support is derived when cards are loaded by the rebuilt application.

## Newly accepted cards

- Accursed Duneyard
- Animal Sanctuary
- Chord of Calling
- Citanul Flute
- Cowabunga!
- Dauntless Dismantler
- Day of Black Sun
- Deepfire Elemental
- Disembowel
- Displacement Wave
- Dominate
- Droning Bureaucrats
- Ent-Draught Basin
- Entrancing Melody
- Farseek
- Fiend Artisan
- Forced March
- Gaze of Granite
- Gorilla Shaman
- Green Sun's Zenith
- Guardian Scalelord
- Hammer Mage
- Hearth Kami
- Here Comes a New Hero!
- Killing Glare
- League Guildmage
- Lin Sivvi, Defiant Hero
- Lupinflower Village
- Meltdown
- Minamo Sightbender
- Molder
- Mudflat Village
- Plaguebearer
- Postmortem Lunge
- Repeal
- Reshape
- Rockface Village
- Sequence Engine
- Serpent of Yawning Depths
- Spawning Kraken
- Spell Blast
- Spell Burst
- Spellstutter Sprite
- Stir the Grave
- Swarmyard
- Unagi's Spray
- Valley Floodcaller
- Valley Mightcaller
- Valley Rotcaller
- Venarian Glimmer
- Vivien's Arkbow
- Wargate
- Whir of Invention
