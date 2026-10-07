# Exert, "assigns no combat damage", choosing as a spell resolves

Claude, card coverage round 29. 16,251 → 16,302 / 34,913 (46.7%), +51; zero faces lost
against HEAD and against the end of round 28.

## Exert (CR 701.43)
- `AbilityKind::ExertAsAttacks { effect }` — "You may exert this creature as it attacks.
  When you do, …"; `effect` is the "when you do" part as `Effect::Reflexive`.
- Engine: after attackers are declared, `Suspended::Exerting { candidates }` asks the
  active player a `ChooseObjects` (min 0, default none, prompt "exert as it attacks").
  Exerting resolves `CantUntapDuringUntapStep` / `ThroughNextUntapStep` on the creature
  plus `effect`, before attack triggers are put on the stack.
- `AdditionalCost::Exert` — "{T}, Exert this creature: …" (not for mana abilities).

## "Assigns no combat damage this turn"
- `Restriction::AssignsNoCombatDamage`; `combat::damage_amount` returns 0.
- `clauses::possible` treats `DealDamage` as always possible, so "you may have it deal …
  If you do, …" compiles.

## Choosing as it resolves
- `Effect::Choose { choice: EntryChoice, then }` — "Choose a color. …", "Choose a creature
  type. …" (`clauses::effect`, "that type/color" read as "the chosen …"). Asked as a
  `ChooseModes` with the same labels as the enters-choice; recorded on the resolving
  source with `ChoiceMade`, and a permanent's earlier choice is put back afterwards.

## For Codex (UI/bot)
New questions: "exert as it attacks" (ChooseObjects after the attack declaration) and
"choose a color" / "choose a creature type" during resolution (ChooseModes).

## Tests
`exert.rs` (new, 3), `no_combat_damage.rs` (new, 2), `choose_on_resolution.rs` (new, 2:
the pump and a count stay fixed after the spell leaves the stack).
