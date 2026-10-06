# Tournament suggestions and deck profiles

In the builder sidebar, choose **Tournament** as the suggestion focus. Open
**Download official MTGO results** and click **Download & update cache** to retrieve
the latest five Modern Challenges. You can instead enter an official
`https://www.mtgo.com/decklist/modern-…` event URL. Downloads run on a background
thread with cancellation; repeated downloads update stable event/player IDs.
Missing cards or invalid source data reject the batch without changing the cache.

 Open
**Import one tournament decklist**, paste a published Modern main deck and
sideboard, and enter a stable result ID, event name, date (`YYYY-MM-DD`), source
URL, and optional placement. Reusing the ID updates that result. Card names must
resolve against the installed card database. Imports reject unknown cards and
invalid metadata before changing the dataset.

For batches, open **Import tournament JSON batch** and paste or drop a UTF-8 JSON
file. **Copy JSON template** provides the required structure. Replace every
placeholder with the actual event and cards. A record has this shape:

```json
[
  {
    "id": "event-id/player-id",
    "event": "Replace with the event name",
    "date": "2026-10-01",
    "format": "modern",
    "source_url": "https://www.mtgo.com/decklist/REPLACE-WITH-EVENT",
    "placement": 8,
    "main": [{"name": "Replace with an installed card name", "count": 4}],
    "sideboard": []
  }
]
```

`placement` and `sideboard` are optional. Quantities must be 1–255; repeated
names/aliases combine with a per-card cap of 255. Main decks cannot be empty.
Dates must be real calendar dates, no later than the current UTC day. Batch IDs
must be unique. Limits: 2 MiB input, 500 results per batch, 2,000 cached results,
and 200 card rows per section. A failing write rolls back the complete batch.
Tournament results are separate from personal saved decks and remain in the
same SQLite database for offline use and backups.

Published lists can be obtained from [official MTGO event pages](https://www.mtgo.com/decklists/)
or [Magic event coverage](https://magic.gg/decklists). The MTGO adapter reads published embedded JSON, with no script execution. It fetches
at most five events per request, restricts requests to official MTGO event paths,
disables redirects, caps responses at 8 MiB, and uses 25-second request timeouts.
It converts MTGO split-card names such as `Left/Right` to oracle names `Left // Right`.
Other providers still use manual imports; no tournament corpus is bundled.

## How recommendations work

The engine compares main-deck quantities using cosine similarity, excluding basic
lands. A score of at least 0.2 establishes a match. It uses Modern observations
from the last 365 days, weights recency with a 90-day half-life, and adds a modest
1.25 multiplier for declared top-eight finishes. Exact repeated lists with the
same source URL and date count as one observation, regardless of their record IDs.
Names and card details are cached during matching.

Suggested cards must currently be Modern legal and fit the draft's color identity.
Main-deck and sideboard observations remain separate. Each suggestion shows
inclusion count, matching-list sample count, rounded average copies among lists
including the card, an observation date range, and up to three source-event links.
Suggestions stop when the corresponding draft section already has that many
copies. Each add button still adds one copy. Sparse drafts and small datasets can
produce broad matches; source links and sample counts help assess those results.

**These counts describe adoption within similar published lists, not win rates.**
Published finishes are selected observations, not the full tournament population.
Tournament mode currently excludes Commander rather than mixing casual Commander,
cEDH, and sixty-card results. Refresh suggestions after another client updates the
local dataset; imports through this screen refresh automatically.

## Mana analysis

The sidebar curve stacks creatures and other spells at mana values 0–6 and 7+.
Hover a bar for exact counts. Land cards are excluded, and quantities come from
the main deck. Split-card costs use the builder's combined mana value. The analysis
shows average spell cost, cheap-spell share, likely draw/ramp/interaction roles,
and colored cost symbols versus potential land sources. Hybrid costs contribute
to both possible colors. Source counts include conditional mana abilities and
are estimates of potential sources, not guarantees of castability or untapped mana.

Opening-seven odds use sampling without replacement before mulligans: zero lands,
two through four lands, and at least one potential source per color. These odds
are omitted for decks smaller than seven or missing card details. Tournament mode
also compares the current curve with average spell counts, land count, and average
spell mana value from similar distinct lists. These benchmarks remain available
when no additions are needed and exclude sideboards.

## Card inspection and pregame profiles

Deck, sideboard, commander, and suggested-add rows show card thumbnails. Hover an
image or name for the rules preview; click to open the persistent enlargement
window. Selected artwork is retained. The whole sidebar scrolls, and offscreen
cards do not request artwork until visible. Adding/removing cards uses the existing
buttons; inspection does not change deck composition.

**Power, competitiveness & Rule 0** edits player declarations: power 1–10,
competitiveness, typical winning turn, infinite combos, extra turns, mass land
denial, fast mana, and free-text expectations. Commander adds a declared bracket:
Exhibition, Core, Upgraded, Optimized, or cEDH. Names were checked against
[official Commander guidance](https://magic.wizards.com/en/formats/commander)
on October 5, 2026; the editor links to current guidance.

These are declarations for table discussion, not automatic power ratings or
bracket certification. Notes can cover proxies and house rules. The compact
profile remains visible in the sidebar; **Copy pregame summary** copies the deck
name and declarations. Profile edits support undo/redo and save atomically with
composition and artwork, including renames. JSON deck export/import carries the
profile; text and CSV formats carry card lists only. Old decks default to an
unrated/undeclared profile. Invalid imported profiles do not replace saved decks.

## Verification

Offline tests cover source parsing, request bounds, URL restrictions, cancellation,
atomic imports, and a 100-card sidebar at widths of 280 and 400 points. These are
headless egui layout/input checks; they do not replace native-window visual review.
The opt-in live test checks the official event schema:

```sh
cargo test -p mtg-app --lib tournaments::mtgo::tests::live_event_schema_still_parses -- --ignored
```

Set `MTGO_TEST_LATEST=1` to exercise latest-five-event discovery. Optionally set
`MTGO_TEST_CARD_SNAPSHOT` to a consistent SQLite backup to test name resolution and
persistence in a temporary copy. The input snapshot remains unchanged; do not use
a copied live database whose uncheckpointed rows remain in a separate WAL file.
