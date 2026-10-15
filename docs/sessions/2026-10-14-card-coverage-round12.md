# Card coverage, round 12 — 2026-10-14

Coverage rises from 17,746 to 17,846 / 34,914 (51.1%) using the same
`/tmp/card-coverage.sqlite` snapshot. The full name comparison gains 100 cards
and loses none. The existing five-card name-lookup difference remains.

Keyword counters are explicit counter state and grant their keyword through
layer 6. Adding another counter of a kind refreshes that kind's timestamp;
removing the last one removes the grant. Zone changes clear both counters and
their timestamps. The UI names the supported keyword counters.

Mixed counter lists use a dedicated IR effect and one simultaneous event batch,
with amounts evaluated before placement. Entering counter choices use the
existing entry-choice pipeline and ask again when a card reenters.

Static bonuses can scale their power/toughness change and then grant independent
keywords. Counts of attachments on an enchanted/equipped object refer to that
object, with a selector-relative attachment filter rather than the Aura or
Equipment itself.

The noun parser tries plural `permanent cards` before the singular prefix.
Conditions support graveyard-card existence and complete `and`/`or` clauses
(with `and` binding more tightly), all living opponents' low-life thresholds,
`you've gained` life thresholds, and source keyword/counter presence or absence.
Source keyword checks remain unsupported in statics because their layer
dependencies are not implemented; Manor Gargoyle deliberately remains blocked.

Eighteen new tests cover gameplay and the conservative compiler guard: keyword
counter timestamps, cleanup, combat, destruction, zone changes, simultaneous
placements, repeated entering choices, scaled Aura and global bonuses,
attachment references, permanent-card filtering, live graveyard thresholds,
compound conditions, multiplayer life checks, repeated attacks, and life-gain
history. No existing fixture was updated.

Targeted tests, `cargo test --workspace --quiet`, Clippy, formatting, and diff
checks pass. Workspace tests run outside the sandbox for their existing loopback
networking tests. Changes remain uncommitted.

## Keyword counters (+25)

Alchemist's Assistant; Avenging Huntbonder; Avian Oddity; Daring Fiendbonder; Duskfang Mentor; Frillscare Mentor; Fully Grown; Heightened Reflexes; Hornbash Mentor; Keensight Mentor; Lyna, Veil of Vengeance; Mai, Jaded Edge; Myojin of Blooming Dawn; Oath of Eorl; Recycla-bird; Sanctuary Smasher; Splendor Mare; Spontaneous Flight; Sudden Spinnerets; Titanoth Rex; Void Beckoner; Vraska Joins Up; Where We're Going...; Wing It; Wingspan Mentor.

## Simultaneous mixed counter lists (+7)

Abigale, Eloquent First-Year; Champion of Dusan; Gift of the Viper; Qarsi Revenant; Sagu Pummeler; Scavenged Brawler; Unexpected Fangs.

## Entering with a chosen counter (+5)

Boot Nipper; Ferocious Tigorilla; Flycatcher Giraffid; Helica Glider; Wingfold Pteron.

## Scaled static bonuses with keyword grants (+12)

Armored Ascension; Auramancer's Guise; Claws of Valakut; Crown of Skemfar; Crystalline Armor; Ethereal Armor; Glaive of the Guildpact; Hold the Gates; Mace of the Valiant; Machinist's Arsenal; Nyxborn Hydra; Ravager's Mace.

## Plural permanent cards (+16)

Basking Capybara; Chupacabra Echo; Coati Scavenger; Council of Echoes; Didact Echo; Echo of Dusk; Exdeath, Void Warlock // Neo Exdeath, Dimension's End; Frilled Cave-Wurm; Join the Dead; Malamet Veteran; Pull Through the Weft; Regenesis; Rydia's Return; Souls of the Lost; Stinging Cave Crawler; Terror Tide.

## Graveyard existence and compound conditions (+26)

Cho-Arrim Legate; Creakwood Safewright; Dawnhand Eulogist; Deepwood Legate; Desert's Hold; Gilded Cerodon; Grond, the Gatebreaker; Igneous Elemental; Kyren Legate; Massacre; Mogg Salvage; Murasa Behemoth; Platypus-Bear; Ramunap Hydra; Refreshing Rain; Rushwood Legate; Sand Strangler; Saprazzan Legate; Sharp-Eyed Rookie; Sidewinder Naga; Snow Devil; Solitary Camel; Submerge; Unquenchable Thirst; Wall of Forgotten Pharaohs; Wretched Camel.

## Further conditions (+9)

Blood Baron of Vizkopa; Bloodghast; Guul Draz Vampire; Haliya, Guided by Light; Momentum Rumbler; Ruthless Cullblade; Sanguine Indulgence; Shatterskull Charger; Vampire Lacerator.
