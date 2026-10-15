# Card coverage, round 13 — 2026-10-14

Coverage rises from 17,846 to 17,871 / 34,914 (51.2%) using the same
`/tmp/card-coverage.sqlite` snapshot. Complete name comparison gains 25 cards
and loses none. The existing five-card name-lookup difference remains.

Additional casting costs may define X by discarding X cards. The announced
value is bounded by the other cards in hand and by variable mana when present;
the actual discard choice uses that X before the spell resolves. Zero is legal,
and answers above the offered maximum are rejected.

Proliferation records an action event even when no counter is added, enabling
triggers such as Scheming Aspirant and Voidwing Hybrid. Repeated proliferations
choose fresh sets and trigger independently. A selected player receives both
energy and poison when both are present. Source attack-history conditions also
support Agent Frank Horrigan and Instill Furor.

Counted noun lists accept singular conjunctions such as `each Aura and Equipment`
and Oxford lists. Shared qualities and controller/zone restrictions apply to the
union, and an object with overlapping types is counted once. Ordinary targeting
noun parsing keeps its previous conjunction rules.

Amplify asks which matching creature cards its controller reveals from hand as
the creature enters. It uses current creature types, including changeling, and
excludes noncreature cards and other players' hands. Revealed cards remain in
hand. Counters are present for state-based actions and enter triggers. Reentry
asks again and may reveal no cards.

Fifteen added gameplay regressions cover variable discard costs and bounds,
proliferation events and repeated choices, energy plus poison, graveyard triggers,
attack history and granted abilities, counted unions, and amplify. Targeted tests,
`cargo test --workspace --quiet`, `cargo clippy --workspace --all-targets -- -D warnings`,
formatting, and diff checks pass. Workspace tests run outside the sandbox for
their existing loopback networking tests. No existing fixture was updated.
Changes remain uncommitted.

## Variable discard costs (+2)

Channeled Force; Sickening Dreams.

## Proliferation and attack history (+7)

Agent Frank Horrigan; Contagion Engine; Ezuri, Stalker of Spheres; Instill Furor; Roalesk, Apex Hybrid; Scheming Aspirant; Voidwing Hybrid.

## Counting conjunctions (+8)

Champion of the Flame; Kellan, the Fae-Blooded // Birthright Boon; Mobilized District; Sarevok, Deceitful Usurper; Thran Power Suit; Toil to Renown; Winter, Tormented Loner; Wyleth, Soul of Steel.

## Amplify (+8)

Aven Warhawk; Canopy Crawler; Daru Stinger; Embalmed Brawler; Feral Throwback; Glowering Rogon; Kilnmouth Dragon; Zombie Brute.
