# Protection qualities — 2026-10-05

The compiler now understands protection from multicolored, monocolored, and colorless sources. Monocolored is expressed as neither colorless nor multicolored, preserving the distinction among zero, one, and multiple colors. Mixed protection qualities joined by “and from” can combine colors and other supported qualities, including card types.

The shared grammar applies to keyword protection and granted protection effects. Existing engine filters and protection handling implement targeting restrictions, blocking restrictions, and damage prevention; no engine implementation changes were needed.

Three regression tests cover keyword/granted parsing and rejection of unsupported exceptions; all nine combinations of protection color count and source color count through targeting, blocking, and resolving a nontargeted damage spell; and mixed protection from red and artifacts.

Coverage against the local database snapshot: 13,983 → 13,992 / 34,913 cards (40.1%). Universal support remains incomplete.

Validation: all-feature oracle suite passed: 80 unit, 457 compiled-game, 35 import, 10 import-atomicity tests (582 enabled total; 15 ignored). Oracle all-target/all-feature Clippy with warnings denied and whitespace checks passed. Test log: `/tmp/mtgo-protection-qualities-tests.log`. Concurrent edits preserved; changes remain uncommitted.
