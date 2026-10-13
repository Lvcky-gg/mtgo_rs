# Card coverage: conditional colors, resolving mana, copies, Channel

Continued Commander staple coverage. Against a fresh HEAD build using the same local
catalog, playable coverage increased **17,424 → 17,468 / 34,914 (50.0%)**. A full
`mtg-cards check` comparison gained 44 card names and lost none. That command reports
17,429 → 17,473 because its exact-name/face lookup differs from `coverage` on five
existing entries; both comparisons have the same gain. The database used for this work
was a SQLite backup in `/tmp`, leaving the user's live catalog untouched.

Newly playable staples: **Pyroblast, Hydroblast, Deathrite Shaman, Culling Ritual,
Sevinne's Reclamation, Otawara, Soaring City, Boseiju, Who Endures**. Other gains include
Eiganjo, Seat of the Empire; Lotus Cobra; Crumbling Vestige; Sarkhan Unbroken;
Pteramander; and Thalia, Heretic Cathar.

Changes:

- A targeted action's `if it's blue/red` suffix checks the announced target's color
  during resolution, using existing selector intersection logic. It does not impose a
  color restriction on choosing the target. The printed modal Blasts compile in full.
- Mana-producing spells and targeted abilities ask for ambiguous output colors on
  resolution. Mana abilities retain the payment planner's existing color choice.
  `Add {B} or {G} for each …` repeats the choice separately for each counted permanent.
  The mana-ability scaling path keeps a `Repeated` output visible to the planner; a
  charged mana-rock payment regression guards against accidentally hiding it in a loop.
  Loyalty abilities now use the same effect parser and can choose their output colors.
- `copy this spell` and the optional new-target suffix compile. Spell copies clear the
  original's casting-zone provenance, so a copy of a flashback Sevinne's Reclamation
  does not satisfy its own graveyard-casting condition and copy itself again.
- `Cost::activation_reduction` evaluates the ability's printed per-object reduction
  before payment. Offering abilities, checking announcement, and paying use the
  evaluated ability cost. Reductions preserve colored requirements and floor generic
  requirements at zero. Channel uses the existing discard-from-hand activation path.
- Noun alternatives can describe `artifact, enchantment, or nonbasic land`; the
  nonbasic qualifier belongs only to the land alternative. `with a basic land type`
  includes nonbasic dual lands. Boseiju's follow-up search names the destroyed
  permanent's controller using last-known information, including stolen permanents.

Added 15 compiled-card engine regressions across `conditional_color.rs`,
`resolving_mana_colors.rs`, `sevinnes_reclamation.rs`, and `channel_reductions.rs`.
The three existing `issue_local_copied_*` fixtures were recorded separately and
reviewed recursively: only the copied object's `cast_context.cast_from` changes from
`Hand` to null, plus the dependent digests. Life, ownership, mana, targets, and zones
retain their previous expectations. The complete replay/regression corpus passes.

Remaining examples: Wishclaw Talisman needs an opponent chosen during resolution;
Delighted Halfling needs the countering protection attached to spending its mana.

Validation: `cargo test --workspace --quiet`, `cargo clippy --workspace --all-targets
-- -D warnings`, `cargo fmt --all --check`, and `git diff --check` pass. The workspace
tests ran outside the sandbox to permit the existing loopback networking tests.
