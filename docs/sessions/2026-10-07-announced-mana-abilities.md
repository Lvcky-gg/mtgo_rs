# Mana abilities with chosen costs, depletion lands, "entered this turn"

Claude, card coverage round 28. 16,189 → 16,241 / 34,913 from this round (+52); 16,251
measured at the end with concurrent work. Zero faces lost against HEAD.

## Mana abilities whose cost is a choice
`mana::announced(cost)`: a mana ability with `TapUntapped` ("{T}, Tap an untapped creature
you control: Add one mana of any color.") or `RemoveCounters { amount: X }` ("Remove any
number of / X storage counters from this land") in its cost.
- Still offered per colour as `Action::ActivateManaAbility` (`manual_source` accepts them).
- Activating one pushes a stack object **without** `AbilityPutOnStack` (so "whenever you
  activate an ability" doesn't see it), recording `CastContext::mana_choice`, and runs the
  ordinary announcement: X (`x_bound` now caps by the counters there, and by mana when the
  cost also has {X}) and cost choices.
- `complete_announcement` → `pay_ability_cost` → `resolve_announced_mana_ability`: the
  object ceases to exist and the effect resolves at once with X, bindings and the colour.
- Never used by automatic payment: such sources are activated by hand (float mana first).
- `pay_ability_cost` pays `RemoveCounters { X }`; `mana::counted` takes `x`.
- Compiler: "remove x …"/"remove any number of …" costs; outputs "add {c} for each
  storage counter removed this way." and "add {c}, then add an additional {c} for each
  charge counter removed this way." (`removed_this_way`). The X cost also works on
  stack abilities (Chamber Sentry).
- Not done: "Add X mana in any combination of {W} and/or {U}" (needs a colour split).

## Smaller pieces
- Depletion lands: condition "there are no <kind> counters on ~" / "~ has no … on it";
  `mana_followup` accepts `If` with follow-up branches and sacrificing the source; "it"
  in a mana follow-up is the source. `Effect::Sacrifice` of `SelfSource` no longer asks
  (a mana ability can't ask, so the sacrifice was silently skipped).
- "Activate only if this land entered this turn or if you control a basic land":
  `activate_only_if` splits " or if " into `Condition::Or`; "~ entered this turn".
- **Bug fixed:** `ObjectFilter::EnteredThisTurn` read `summoning_sick`, which lasts until
  its controller's next turn. New `GameObject::entered_turn` (set on entering the
  battlefield and on token creation), used by eval and detect.
- conditions.rs: the "there are …" graveyard branch returned early with `?`; now falls
  through.

## For Codex (UI/bot)
`ActivateManaAbility` can now lead to an announcement (ChooseObjects / ChooseX) before
the mana arrives; the bot never sees these sources in automatic payment.

## Tests
`announced_mana.rs` (new, 4): tap-a-creature mana (summoning-sick creature pays), storage
land X=2, battery X=2 → 3 mana, X-damage ability with X capped at 2 by mana.
`mana_followups.rs` (+2): depletion land sacrificed with its last counter; land usable the
turn it entered, not the next (still summoning sick), and again with a basic land.
Engine + Oracle tests pass (Oracle compiled 768, 15 existing ignored); Clippy clean.
