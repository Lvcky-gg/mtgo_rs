# Autonomous app improvements

Completed the following app, deck-storage, and card-art improvements during the
authorized autonomous run. Concurrent engine/compiler edits were preserved.

## Match controls and responsiveness

- Independent selections and confirmation for multiple target slots, preserving
  slot order, legal repeated targets, and empty slots.
- Exact-count cleanup discard controls, through hand cards or named buttons.
- Optional single-mode questions allow selecting no mode when the minimum is zero.
- Named blocker-order controls with explicit confirmation.
- Observer processing capped at 128 positions per frame, with immediate repaint
  and earlier views drained before delivering a newer question.
- Undo/pass shortcuts respect focused text and numeric inputs.

## Deck integrity

- Atomic saved-deck replacement and rename through a SQLite transaction. Failed
  writes preserve the previous saved deck and the edited draft.
- Draft conversion reports missing cards, read failures, and oversized counts
  instead of silently dropping or reducing entries. Saved-deck conversion also
  rejects oversized counts.
- Companion and multiple-commander imports fail before saving because the app
  cannot preserve those roles in its current deck representation.
- Builder loading validates commander cardinality and checks duplicate-row sums.
  Import summaries check total counts before replacing any saved deck.
- Deck deletion reports failures and preserves a different selected deck.

## Card art

- Decoder dimension and allocation limits supplement the compressed-byte limit.
- SHA-256 cache filenames distinguish punctuation variants and bound path length.
- Exclusive temporary files prevent concurrent workers from truncating one
  another's cache writes; failed writes clean up their own temporary files.
- Cached data must decode successfully. Corrupt caches fall through to download,
  downloaded data is decoded before caching, and local reads have a byte limit.
- Module documentation now matches the shared worker pool and API request gate.

## Validation

`cargo test -p mtg-app -p mtg-store --all-targets --quiet` passes:

- 149 app library tests.
- Both network-match integration tests, including a hosted match played to completion.
- All 17 store tests.

Total: 168 passing tests. GUI build, app/store all-target Clippy with dependency
lints disabled and warnings denied, and diff checks pass. Local socket tests used
the approved cargo-test permission outside the sandbox. No interactive visual
inspection was performed.

## Compatibility notes

Draft::spec now returns Result; all workspace callers are updated. The app adds a
direct sha2 0.11 dependency already present through mtg-net. Old ambiguous image
cache files remain on disk but are not reused; requested images are downloaded
under the new full-name digest paths. No commits were created.

Focused task handoffs are available in this directory; docs/coordination.md records
each change and its checks.
