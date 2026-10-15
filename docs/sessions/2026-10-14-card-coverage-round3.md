# Card coverage, round 3 — 2026-10-14

Using the same `/tmp/card-coverage.sqlite` snapshot, coverage rises from 17,616
to 17,644 / 34,914 (50.5%). A complete card-name comparison gains 28 cards and
loses none. The name-check command retains its existing five-card difference
(17,649 versus 17,644).

## Typecycling

Typecycling accepts known subtypes, singular card types, and artifact lands,
using the existing discard-from-hand activation, reveal, search, and shuffle
machinery. It does not draw a card. The artifact land filter requires both types.

Newly playable: Hobbit Hole; Sojourner's Companion; Step Through;
Vedalken Aethermage.

## Base power/toughness and added subtypes

Setting base power/toughness no longer ends the modification parser before
following predicates. Plural subjects can gain subtypes with `are … in addition
to their other types`, including a subsequent granted ability. Existing types
remain intact. Basic land type additions remain unsupported here: adding those
subtypes also requires intrinsic mana abilities, which this path does not grant.

Newly playable: Almost Perfect; Creeperhulk; Eidolon of Astral Winds;
Gigantiform; Gourmand's Talent; Kudo, King Among Bears; Ragost, Deft Gastronaut;
Rin and Seri, Inseparabler; Senator Peacock; Taranika, Akroan Veteran;
Valentina Allegra de Fontaine; Water Wings.

## Numbered toxic grants and tokens

The shared keyword parser accepts toxic values 1–255, including keyword lists
for temporary grants, static grants, and token definitions. It rejects missing,
zero, negative, out-of-range, and malformed values. Multiple identical toxic
grants remain distinct in layer evaluation so their values add, along with any
printed toxic abilities.

The Mite regression exposed an existing blocking validation gap: `can't block`
was honored by the candidate list but not by direct block validation. The check
now lives in the shared pair validator, so both paths enforce it.

Newly playable: Aspirant's Ascent; Basilica Shepherd; Charge of the Mites;
Crawling Chorus; Indoctrination Attendant; Infested Fleshcutter;
Karumonix, the Rat King; Mirrex; Mite Overseer; Necrogen Communion;
Prosthetic Injector; Stinging Hivemaster.

Seven compiled gameplay regressions cover two typecycling filters and their
reveal/shuffle/cycling events without drawing; Kudo's sizes and preserved creature
types; Water Wings' expiration; Senator Peacock's Clue type and sacrifice draw;
stacking printed and identical granted toxic; Aspirant's Ascent's expiration; and
a Mite's combat poison and blocking restriction. Earlier Archon/Smallpox tests
now inspect actual battlefield objects rather than querying the shared battlefield
with a player-specific zone reference.

Validation: targeted compiled tests; `cargo test --workspace --quiet` including
the replay/regression corpus; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all --check`; and `git diff --check`. Workspace tests run outside
the sandbox for their existing loopback networking tests. Changes remain uncommitted.
