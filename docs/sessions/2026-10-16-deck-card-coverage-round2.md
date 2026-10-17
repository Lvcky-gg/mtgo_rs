# Deck card coverage — 2026-10-16, round 2

All 209 / 209 saved deck priorities now compile. Newly supported in this round:
Cursed Mirror, Preston, the Vanisher, and Slimed. The refreshed report is
`docs/card-coverage/deck-priorities-report.txt`. Starting priority coverage was
206 / 209, after this day's Phelia work, which remains preserved and uncommitted.

Whole-catalog compilation coverage on the same SQLite snapshot is now
18,921 / 34,954 (54.1%), up from 18,909. Compilation coverage is not a complete
rules audit or a per-card global regression comparison.

## Copy behavior

Copy exceptions are now applied in layer 1, before entry triggers are observed,
and are inherited by subsequent copies. The exceptions are carried on the object
for last-known copy information; ordinary pumps, counters, and granted abilities
remain outside its copiable values. The new state fields omit empty/default
values in verification snapshots. Existing IR without the new optional fields
remains readable.

Preston distinguishes creatures that were cast from those put onto the
battlefield by effects. Its tokens are 0/1 white Illusions, replacing the old
creature types while retaining other card types such as artifact. A copied
power/toughness-defining static ability is omitted when the copy specifies 0/1.
The tokens retain other abilities and do not trigger Preston recursively.

Cursed Mirror can decline copying. If it copies, it has haste and the copied
creature's entry behavior; at cleanup it becomes Cursed Mirror again with its
mana ability. A subsequent clone keeps the copied creature and haste permanently.
Blinking returns the actual Mirror card and offers a fresh copy choice. Entry-copy
choices also run in the resolution paths for ordinary zone moves, reanimation,
and dig effects rather than only when casting a permanent spell.

## Counter behavior

Counter instructions bind the battlefield sources of abilities actually
countered. Slimed uses that binding to disable all activated abilities of that
permanent, including mana abilities, until cleanup. It leaves other permanents
alone, does not disable a source if the ability cannot be countered, and does
not follow a source through a later zone change.

## Regression coverage

Ten copy gameplay tests cover temporary copy expiration, optional copying,
copies of Mirror keeping haste, Preston ignoring cast/opponent creatures,
Illusion characteristics and copied entry abilities, copies retaining exceptions
without copying pumps, blinking/bouncing Mirror, copied characteristic-defining
power/toughness, entry triggers observing 0/1, and sacrificing five of Preston's
Illusions to activate its exile ability.

Four Slimed tests cover countering activated and triggered abilities, mana and
nonmana activation restrictions, cleanup expiration, independent sources,
uncounterable abilities, and a source leaving and returning with a new identity.

Changes remain uncommitted.

Validation: all workspace tests pass outside the sandbox (the first sandboxed
attempt was denied local socket binding in the existing networking tests).
The final focused copy and Slimed regressions pass, as do
`cargo check --workspace --all-targets`, compiler-library Clippy with warnings
denied, formatting, and `git diff --check`. Engine-library Clippy passes with
only the previously documented `collapsible_if` lint allowed. The existing full
workspace/all-target lint debt remains as recorded in the earlier session.
No existing replay or regression fixture was updated.
