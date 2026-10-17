# Legendary-creature coverage — 2026-10-16

Expanded the target from the completed 209-card deck list to every local card
record having at least one face that is both Legendary and Creature. This
includes legendary artifact/enchantment creatures, back faces, and digital or
otherwise unusual cards. No format-legality filtering or unsupported-layout
exclusions are applied to the target.

The snapshot contains 4,025 matching oracle records and 4,024 distinct names.
Two different oracle records are named Joven and Chandler. The plain-name
inventory is `docs/card-coverage/legendary-creatures.txt`; the authoritative
record-level blocker report is `docs/card-coverage/legendary-creatures-report.txt`.
The report preserves oracle UUIDs and actual blocker text so duplicate names
remain distinct. Embedded line breaks and tabs are escaped to keep one record
per report line. Compilation covers every face and requires a supported layout.

Legendary-creature compilation coverage increases from **1,116 to 1,161 / 4,025**
(28.8%): **45 newly supported records, zero regressions in the scoped identity
comparison, 2,864 still blocked**. This is the first batch toward all legendary
creatures, not completion of that larger target. Compilation coverage is not a
complete rules audit. The original deck priorities still compile 209 / 209.

## Reproduce

Use a SQLite backup of the local database, e.g. `MTGO_RS_DB=/tmp/card-coverage.sqlite`:

- `cargo run -q -p mtg-headless --bin mtg-cards -- coverage --legendary-creatures --names`
- `cargo run -q -p mtg-headless --bin mtg-cards -- coverage --legendary-creatures --blocked`
- `cargo run -q -p mtg-headless --bin mtg-cards -- coverage 50 --legendary-creatures`

`--names` emits a sorted, deduplicated checklist compatible with `check <file>`.
Use the direct scoped coverage/report command for record-level totals; the
name-based checker cannot distinguish the two Joven and Chandler records.

## Shared rules

- Conditional creature-type removal for devotion gods, including removal and
  restoration of their creature subtypes as devotion changes. Hybrid symbols
  count once for two-color devotion; opponents' permanents do not contribute.
- Flavor labels before activated mana costs, while preserving rules-bearing
  labels such as Boast, Exhaust, and Ward.
- Printed hexproof from a quality, including artifacts and enchantments, using
  the existing source-filtered targeting restriction. Friendly sources remain
  able to target the creature.
- Prowess alongside printed keyword lists, and granting melee to other creatures.
- Teferi's flash grant to owned creature cards in every nonbattlefield zone,
  without granting permission to play cards from those zones.
- Aeve's removal of legendary from itself only when it is a token.
- Death-trigger multipliers for Teysa Karlov and Drivnod. Modifiers use the state
  before death, including doublers dying in the same batch, and preserve a stolen
  creature's battlefield controller. Previously departed doublers and doublers
  still on the stack do not apply. Multiple extra-trigger effects add one each.
- Sums of independently described quantities, largest opposing hand size,
  combined hand sizes, toughness-only definitions, and combined enter/die triggers.

Review also found a Preston edge case from the prior round: its trigger failed
its cast-history condition if the entering creature left before resolution.
Conditions on prior selected objects now consult last-known properties, and a
new regression verifies the trigger still creates its 0/1 copy.

Eighteen new compiled gameplay regressions cover these shared rules and their
important boundaries. A CLI regression checks that legendary and creature must
appear on the same face, including a qualifying back face and a subtype whose
name is Legendary.

## Newly supported records

- Abomination of Llanowar
- Adamaro, First to Desire
- Adriana, Captain of the Guard
- Adric, Mathematical Genius
- Aeve, Progenitor Ooze
- Armix, Filigree Thrasher
- Brawn, Amadeus Cho
- Captain Marvel, Earth's Protector
- Commissar Severina Raine
- Daxos, Blessed by the Sun
- Drivnod, Carnage Dominus
- Erebos, Bleak-Hearted
- Erebos, God of the Dead
- Flatman
- General Ferrous Rokiric
- Goliath, Mass Manipulator
- Heliod, God of the Sun
- Heliod, Sun-Crowned
- Human Torch, Johnny Storm
- Immortus, Master of Eternity
- Inquisitor Greyfax
- Iron Fist, Hero for Hire
- K-9, Mark I
- Karametra, God of Harvests
- Marneus Calgar
- Molly Hayes, Runaway
- Monica Rambeau // Photon, Living Light
- Multani, Maro-Sorcerer
- Namora, the Sea Queen
- Nylea, God of the Hunt
- Oswald Fiddlebender
- Pharika, God of Affliction
- Phenax, God of Deception
- Purphoros, God of the Forge
- Ruric Thar, Biomagus
- Shang-Chi, Martial Mentor
- She-Hulk, Jade Defender
- Teferi, Druid of Argoth
- Teferi, Mage of Zhalfir
- Teysa Karlov
- Thassa, Deep-Dwelling
- Thassa, God of the Sea
- The Bearded Teakeeper
- Thor Odinson
- Titania, Proud Pummeler

Changes remain uncommitted; the earlier deck coverage work is preserved.

## Validation

`cargo test --workspace --quiet` passes outside the sandbox, including the
existing loopback tests, the new CLI scope test, and all new gameplay regressions.
Scoped library/CLI Clippy passes with warnings denied and only the previously
recorded `collapsible_if` lint allowed:
`cargo clippy -p mtg-engine -p mtg-oracle -p mtg-headless --lib --bin mtg-cards -- -D warnings -A clippy::collapsible_if`.
Formatting and `git diff --check` pass. Existing replay/regression fixtures were
not updated. The existing full-workspace/all-target lint debt remains unchanged.
