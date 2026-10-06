# Deck suggestions, analysis, and interchange

User prioritized suggestions, synergy, and deck analysis within the wider Archidekt-style builder request.

Implemented theme-based candidate searches across the card database, exact normalized token scoring, reduced generic rules-word noise, and Commander color identity filtering based on the commander even when illegal draft cards are present. Searches remain bounded to 200 candidates per theme; recommendations are lexical estimates rather than a curated combo database.

Added copy-weighted main-deck analysis: land count, average spell mana value, estimated draw/ramp/interaction counts, colored cost symbols (hybrids count toward both colors), potential land sources, and missing-source notices. Conditional sources are explicitly labeled; sideboard and commander do not inflate main-deck analysis.

Added CSV and JSON adapters alongside existing text formats. CSV supports quoted names, escaped quotes, embedded newlines, optional sections and printing hints; JSON supports an entries array or root array. Invalid quantities and sections remain visible as import errors. Native import accepts a dropped UTF-8 file capped at 2 MiB; export copies the selected format. This does not implement arbitrary proprietary binary formats, MTGO XML files, account integration, or full Archidekt feature parity.

Validation: 198 app tests passed with local loopback socket access before the analysis regression was added; all 42 builder tests subsequently passed, including the new analysis and recommendation regressions. All 27 deck tests passed. App/deck all-target Clippy passed with warnings denied. Export roundtrip tests cover all seven formats.

## Continuation: missing-role recommendations and spreadsheets

Recommendations now query draw, mana acceleration, and interaction candidates when those roles are absent from the main deck. Shared role classification drives both analysis and scoring. Reasons identify the missing role being filled; regression coverage confirms those bonuses disappear after adding the roles. Removed the unconditional legendary-creature bonus because legendary status alone does not indicate commander synergy.

CSV parsing also accepts tab-separated spreadsheet tables, including whitespace in headers and CRLF. Native import help lists TSV. Validation: all 42 builder tests and all 28 deck tests pass; app/deck all-target Clippy with warnings denied and whitespace checks pass.

## Continuation: focus modes and opening-hand odds

Added a recommendation focus selector: Balanced preserves mixed scoring; Synergy suppresses missing-role and curve bonuses; Missing roles increases gap-filling priority; Early curve restricts results to nonlands with mana value at most two. Focus is part of recommendation cache identity, so switching updates results immediately.

Deck analysis calculates hypergeometric probabilities for an opening seven containing zero lands or two through four lands, from actual main-deck counts before mulligans. Decks smaller than seven omit these values. Exact small-deck fractions, probability normalization, all-land/no-land boundaries, and mode ranking/filtering have regression coverage.

Validation: 43 builder tests passed; app all-target Clippy with warnings denied and whitespace checks passed.
