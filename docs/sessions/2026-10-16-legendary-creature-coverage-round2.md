# Legendary-creature coverage, round 2 — 2026-10-16

The local snapshot now compiles **1,174 / 4,025 legendary-creature oracle
records (29.2%)**, up 13 from 1,161. Comparing oracle UUIDs against the previous
report finds zero scoped regressions. **2,851 records remain blocked**; this
batch does not complete the all-legendary target. Compilation coverage is not
a complete rules audit. The scope and duplicate-name caveat remain documented
in the first legendary-creature session note.

## Shared mechanics

- Firebending creates an attack trigger that adds red mana retained until combat
  ends. Fixed values and the printed power-based X definition are supported.
  Resolution uses current power or last-known power if the source left.
  Printed keyword lists, granted firebending, and token keyword text are handled.
- Combat-retained mana is tracked separately from Birgi's turn-retained mana.
  Spending retained mana removes its retention, so later ordinary mana cannot
  inherit it. Payment also distinguishes restricted mana left unspent.
- Printed Mayhem permits casting that particular discarded card from the
  graveyard for its alternative mana cost during the discard turn. Ordinary
  timing still applies, including flash. Discarding as an activation cost works;
  milling and other copies in the graveyard do not qualify. Leaving the
  graveyard resets permission after the spell resolves; Mayhem does not exile it.
- Granted Mayhem remains unsupported. Its card-wide grant needs separate work;
  cards containing that text remain blocked rather than being approximated.

The official [Spider-Man release notes](https://magic.wizards.com/en/news/feature/spider-man-release-notes)
confirm Mayhem's alternative cost, same-turn discard requirement, normal timing,
and continued payment of additional costs.

Fourteen new gameplay regressions cover Firebending (seven) and Mayhem (seven).
They include mana expiration, spending and restricted-mana interactions,
last-known power, mixed keyword lists, temporary grants, tokens, card identity,
resolution, timing, flash during an opponent's turn, discard costs, and mandatory
additional spell costs.

## Newly supported records

- Abomination, World Ravager
- Azula, On the Hunt
- Fire Lord Azula
- Green Goblin, Back for More
- Iroh, Dragon of the West
- Mai and Zuko
- Swarm, Being of Bees
- The Legend of Roku // Avatar Roku
- The Squadron Sinister
- Ultimate Green Goblin
- Uncle Iroh
- Zhao, Ruthless Admiral
- Zuko, Exiled Prince

## Reports

`docs/card-coverage/legendary-creatures-report.txt` contains the current totals
and every remaining blocker with its oracle UUID. Reproduce with a backup of
the local card database:

```sh
MTGO_RS_DB=/tmp/card-coverage.sqlite cargo run -q -p mtg-headless --bin mtg-cards -- coverage --legendary-creatures --blocked
```

All changes remain uncommitted, including the preceding coverage batches.

## Validation

- `cargo test --workspace --quiet` passed (existing ignored tests remain ignored).
- Final focused Firebending and Mayhem tests passed, including the added
  additional-cost regression and the final graveyard-cost ordering adjustment.
- Scoped Clippy passed with `-D warnings -A clippy::collapsible_if`; the allowance
  is for existing unrelated lint debt documented in the preceding session.
- `cargo fmt --all --check` and `git diff --check` passed.
- Original deck-priority compilation remains **209 / 209**.
