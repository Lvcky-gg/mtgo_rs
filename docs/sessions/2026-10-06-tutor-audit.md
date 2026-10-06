# Tutor audit

Audited all 1,192 distinct stored card-face records whose text contains both
`search` and `library`, using the installed oracle database through a read-only,
immutable SQLite connection. This deliberately broad set includes search prevention,
opponent-library searches, reminder text, and unusual card sets. Records are
unique by name, type line, mana cost and oracle text; these are not counts of
unique ordinary tutor cards.

442 faces compile completely; 750 have at least one unsupported line. Complete
compilation is a coverage measurement, not proof that every card was individually
played. Regression tests exercise the shared search implementations and existing
landcycling, transmute and activated-ability paths.

## Fixed during the audit

- Reveal instructions were discarded by the tutor parser. Searches now select
  once, reveal those selected cards, then perform the printed destination and
  shuffle operations. Top-of-library searches reveal before shuffling.
- Public reveal history crosses the projected-view boundary as card identities
  and owners only. Both clients announce reveals in the match journal. Hidden
  hand identities, library object IDs and library order stay redacted. The
  history supports repeated reveals without repeating earlier announcements.
- Revealing prevents undo across newly disclosed information.
- Duplicate object IDs in malformed answers are deduplicated before enforcing
  selection limits and topping up mandatory choices. A two-card tutor cannot
  count the same card twice.

The earlier Gamble fix remains covered: the selected card enters hand before
random discard, either the selected card or an existing card can be discarded,
and seeded replays agree.

## Validation

Search tests cover hand, battlefield, tapped battlefield, graveyard, exile and
library-top destinations; mandatory, optional, filtered and multiple-card
selection; empty and short libraries; no matching cards; shuffle on failed
search; reveal ordering and opponent visibility; source sacrifice; named cards;
and random-discard sequencing. Rendered egui interaction verifies that mandatory
Confirm waits for a selection and optional Confirm can submit no cards.

Native screenshots use the real renderer with a 25-card library selection and
an additional card arriving in hand. These presentation fixtures do not simulate
rules resolution; engine tests provide that separate evidence.

Checks passed:

```sh
cargo test -p mtg-app -p mtg-oracle -p mtg-engine -p mtg-policy -p mtg-ir --offline --quiet
cargo test -p mtg-session -p mtg-net --offline --quiet
cargo test -p mtg-oracle --test compiled search --offline --quiet
cargo clippy -p mtg-oracle -p mtg-engine -p mtg-ir -p mtg-app --all-targets --offline -- -D warnings
cargo build -p mtg-app --example visual_states --bin mtg-gui --offline
```

The broad suite passed 1,249 tests with 43 ignored; session/network checks passed
90 additional tests. The final focused search run passed 17 matching tests,
including two unrelated tests whose names also contain `search`.

## Unsupported patterns still reported

| Examples | Missing support |
| --- | --- |
| Cultivate, Kodama's Reach | Split chosen cards between battlefield and hand |
| Farseek | Comma-separated four-subtype search filter |
| Chord of Calling, Green Sun's Zenith | Variable mana-value search bounds; Zenith also has a self-shuffle instruction |
| Other rejected faces | Opponent libraries, different-name constraints, complex choices, unrelated unsupported abilities |

These cards remain rejected by the strict playability gate. This audit does not
add all 750 rejected faces or validate every other ability on catalog cards.

## Reproducing compiler coverage

`crates/mtg-oracle/examples/audit_tutors.rs` reads JSON containing `faces` (name,
type_line, mana_cost, oracle_text) and a subtype-name-to-ID map. Export records
with `SELECT DISTINCT name,type_line,mana_cost,oracle_text FROM faces WHERE
lower(oracle_text) LIKE '%search%library%' ORDER BY name`, and subtype IDs with
`SELECT name,id FROM subtypes`. The installed SQLite database should be opened
read-only with `immutable=1` when exporting, so this audit cannot modify it.

```sh
cargo run -p mtg-oracle --example audit_tutors --offline -- /tmp/mtgo-tutors.json
cargo run -p mtg-app --example visual_states -- /tmp/mtgo-tutor-visual
```

The GUI was rebuilt. Start a new match with the updated executable; imported
cards compile from stored text when loaded, so decks need not be reimported.
