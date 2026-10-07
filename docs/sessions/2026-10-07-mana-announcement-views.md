# Mana announcement views

Announced mana abilities use an internal stack-zone object to collect chosen costs.
Player views previously exposed that bookkeeping object as a stacked ability.
Projection now excludes objects whose cast context has mana_choice, and builds
the public stack from projected objects. No announcement or payment logic changed.

The real-engine regression covers creature-tap and counter-X choices, with an
empty stack and a real spell already on it, for both players. Even explicitly
listing the internal object as shown does not expose it. The source, helper,
and real spell remain visible. Answering costs produces the correct mana
immediately and removes the internal object while retaining the real spell.

Validation: focused regression passes; all 230 app library tests pass with five
existing ignored. Engine/Oracle all-target Clippy with warnings denied and diff
whitespace checks pass. The broad engine/Oracle run encountered the concurrently
added exert::exert_as_a_cost_keeps_the_creature_tapped_through_the_next_untap
failure at its final untap assertion (775 other compiled tests passed, 15 ignored).
That test does not call player projection; concurrent exert code was preserved.
All 270 engine tests pass. Rerunning Oracle with the exert test excluded passes
903 tests, with 15 existing ignored; combined with the app run, 1,403 tests pass.

Files: engine/view.rs, compiled/announced_mana_view.rs and registration, handoff.
Changes remain uncommitted.
