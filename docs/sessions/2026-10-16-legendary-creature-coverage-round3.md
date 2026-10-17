# Legendary-creature coverage, round 3 — 2026-10-16

The local snapshot compiles **1,210 / 4,025 legendary-creature oracle records
(30.1%)**. This batch adds 37 supported records and corrects one previously
reported false positive, for a **net gain of 36** over 1,174. **2,815 remain
blocked**. The all-legendary target is still incomplete; compilation coverage
is not a complete rules audit. The same face/layout scope and oracle UUID
comparison described in the first session note apply.

## Shared rules and parser coverage

- Distinct-power mana production for Loot and Selvala, using current powers,
  deduplicating equal values, and counting zero and negative powers. The chosen
  color applies to all mana produced; automatic spell payment can use it.
- Karn/Sydri mana-value artifact animation: the target must be a noncreature
  artifact. It becomes an artifact creature with base power/toughness fixed at
  resolution, retaining its color and abilities until the turn ends. Animating
  an unprotected zero-mana-value artifact normally sends it to the graveyard.
- Gendered object pronouns (he/she/him/her) use the compiler's contextual object
  referent, including event subjects and previous targets. Word boundaries keep
  these pronouns from consuming the beginning of another word.
- Existential condition contractions ("there's a Lesson card ..."), conditional
  untap-restriction suffixes ("... if you control a Reflection"), and the combined
  enters-tapped/does-not-untap template used by Traxos and Grimgrin.

## Power-up correction

Reviewing the newly compiled gendered text exposed an existing problem:
Power-up had been stripped as a flavor label. That silently lost its actual
keyword rules. It is now protected from flavor stripping and compiles with a
new activation timing. Each ability is usable once per object, tracked by the
existing lifetime activation history. Countering the ability does not refund
that use; a new object after blinking has a new allowance.

On the entry turn, the permanent's current mana cost reduces the activation
cost. Generic reduces generic, colored/colorless reduces the matching type,
and excess reduces generic; X in a permanent's mana cost is zero. This
reduction applies after cost increases. These rules follow the official
[Marvel Super Heroes release notes](https://magic.wizards.com/en/news/feature/marvel-super-heroes-release-notes)
and [Comprehensive Rules, 702.193](https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf).

Hybrid/Phyrexian reduction choices need further announcement support and remain
blocked. **Brawn, Amadeus Cho** was previously reported supported through
flavor stripping and is now correctly blocked. This is the sole newly blocked
oracle record in the comparison; it is an intentional correction. Twelve
legendary records with Power-up now compile with its actual rules, including
previously counted Captain Marvel, Human Torch, Iron Fist, and Goliath.

## Newly supported records

- Aang, A Lot to Learn
- Alirios, Enraptured
- Black Panther, Most Dangerous
- Boomerang, Blade Flinger
- Bosco, Just a Bear
- Crossbones, Malicious Mercenary
- Darwin, Adaptive Mutant
- Dáin of the Ancient Halls
- Edgar, Ancient Bloodlord
- Elektra, Femme Fatale
- Gandalf, Spark Starter
- Ghost-Spider, Gwen Stacy
- Grimgrin, Corpse-Born
- Hercules, Olympian Hero
- Hercules, Prince of Power
- Jack of Hearts, Volatile Hero
- Jet, Freedom Fighter
- Kang, Temporal Tyrant
- Karn, Silver Golem
- Killmonger, Ruthless Usurper
- Loot, the Nexus
- Luke Cage, Power Man
- Machine Man, Model X-51
- Madame Masque
- Metalhead
- Mockingbird, Bobbi Morse
- Momo, Rambunctious Rascal
- Scarlet Spider, Kaine
- Selvala, Eager Trailblazer
- Shocker, Unshakable
- Smaug the Magnificent
- Sydri, Galvanic Genius
- The Wasp, Janet Van Dyne
- Tiger Shark, Abyssal Hunter
- Traxos, Scourge of Kroog
- Venus, Torn Between Worlds
- Zuko, Seeking Honor

## Reports and validation

The current record-level report is
`docs/card-coverage/legendary-creatures-report.txt`. Reproduce using a backup
of the local card database:

```sh
MTGO_RS_DB=/tmp/card-coverage.sqlite cargo run -q -p mtg-headless --bin mtg-cards -- coverage --legendary-creatures --blocked
```

Fifteen new focused regressions cover gameplay boundaries and conservative
rejection of hybrid Power-up text. This batch is included with the accumulated coverage work in the latest build
push requested by the user.

Validation completed for the final code:

- Full compiled-card suite: 1,564 passed, 15 existing ignored tests.
- `cargo test --workspace --all-features --locked --quiet` passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
  passed with no lint allowances after the pipeline fixes.
- `cargo fmt --all --check` and `git diff --check` passed.
- Original deck-priority compilation remains **209 / 209**.
- Optimized Linux desktop build completed successfully.
