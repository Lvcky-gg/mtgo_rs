# Conditions, and "other" after making tokens

Claude, card coverage round 47. 16,855 → 16,895 / 34,913 (48.4%), 44 faces, all from
this round. Zero faces lost against HEAD and round 46.

Method: every "as long as / if <condition>," in the not-understood lines, probed as
"As long as <condition>, this creature gets +1/+1." — failures that are real conditions
(not "you do", coin flips, reveal results) were added to `conditions::condition`:

- "~ is an enchantment", "you gained N or more life this turn"
  (`LifeGainedThisTurn`), "you have a full party" (`PartySize` ≥ 4), "X is N or more"
  (only while `cx.x`), "there are N or more <noun> cards in your graveyard" (spell
  mastery, threshold-like counts by type).
- The "you have …" branch returned `None` when no number followed, hiding later "you
  have …" conditions; it now falls through.
- Not done: revolt (needs control as computed by layers when a permanent leaves), "an
  opponent controls more lands than you" (per opponent, not summed), coven.

Fix: `nouns::mass` read "all other creatures" as "other than the source", so Martial
Coup's "Create X Soldiers. If X is 5 or more, destroy all other creatures." destroyed the
Soldiers. `Cx::made_tokens` (set once the effect created tokens, which the engine binds as
"it") makes "other" also exclude `InBinding(It)`.

Tests: `create_number.rs` (Martial Coup keeps its five Soldiers), `value_phrases.rs`
(spell mastery static; "gained 3 or more life this turn" end-step trigger).

## Round 50 addendum

16,970 → 16,991 (22 faces): coven (`Value::DistinctPowers` ≥ 3 among creatures you
control), "this spell was cast from a graveyard" (`WasCast(Graveyard)`), "~ is on the
battlefield", "your speed" (`Value::Speed`), "half your starting life total", "that
permanent's …". The "you control …" branch now falls through when `controls` fails.

Engine fix: `Condition::WasCast` only read `GameObject::cast_from`, which a spell on the
stack doesn't have; it now falls back to `cast_context.cast_from` (a flashback test drew
one card instead of two).
