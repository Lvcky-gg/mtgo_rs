# Tournament suggestions, mana analysis, card inspection, and deck profiles

User authorized tournament-data recommendations, then added mana curves, sidebar
card imagery/popups (including suggestions), and power/Rule 0/bracket/competitiveness.
All four requests are implemented in the native deck builder.

Tournament mode imports Modern text decklists with event metadata or JSON batches.
Results persist separately from personal decks in SQLite. Imports validate all rows,
resolve card names, merge by stable ID, and commit atomically. Recommendations use
nonbasic quantity cosine similarity, recent observations (365 days), 90-day recency
half-life, and a modest top-eight placement multiplier. Exact same-source/date/list
observations deduplicate; main and sideboard counts remain separate. Suggestions
show inclusion/sample counts, rounded typical quantities, dates, and event links.
Basic-land-only drafts do not establish matches. Current Modern legality and draft
color identity filter candidates. This phase imports supplied data; no website
crawler or tournament corpus is bundled. Adoption is explicitly distinguished from
win rates, and Modern results cannot drive Commander recommendations.

Mana analysis stacks creatures/other spells and exposes hover counts, cheap-spell
share, opening-seven land/source odds, and a comparison with similar tournament
lists' mana curve, average land count, and spell cost. Unknown card details visibly
mark incomplete analysis and suppress odds. Conditional sources remain labeled.

Commander, main, sideboard, and suggested rows have card thumbnails, hover previews,
and direct persistent enlargement on image/name click. Artwork preferences are
preserved. A single scroll area keeps the whole sidebar accessible; offscreen card
tiles do not request images. A pointer regression proves inspection does not add
cards and that enlargement retains artwork identity.

Saved profile declarations include power 1–10, competitiveness, winning turn,
Commander bracket, infinite combos, extra turns, mass land denial, fast mana, and
Rule 0 notes. Bracket names checked against official Commander guidance on October
5, 2026. Ratings/brackets are explicitly player declarations, not inferred or
certified ratings. Profiles support history, dirty tracking, atomic save/rename,
legacy API preservation, and JSON export/import. Invalid profile imports do not
replace existing saves. Text/CSV exports retain card-list semantics.

Schema v9 adds tournament records; v10 adds default-empty deck profiles. Existing
migration preservation and store tests pass. Operational details and import schema:
`docs/tournament-data.md`.

Validation: full app suite 208 passed, 0 failed, 3 ignored (loopback access enabled);
all 27 store tests passed; app/store all-target Clippy with warnings denied passed;
whitespace checks passed. Includes atomic-write fault injection, duplicate updates,
date validation, recency/legality/section matching, curve benchmarks, popup clicks,
and profile persistence/export/import regressions. No live user database was changed
and no external communications or commits were made.
