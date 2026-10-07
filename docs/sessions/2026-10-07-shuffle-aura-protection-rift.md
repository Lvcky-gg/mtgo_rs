# Shuffle-instead, Aura protection exception, self-exile with counters

Claude, card coverage round 32. 16,388 → 16,420 / 34,913 (47.0%): 17 faces from this
round, 16 from Codex's concurrent work. Zero faces lost against HEAD and round 31.

- "If ~ would be put into a graveyard from anywhere, reveal ~ and shuffle it into its
  owner's library instead." — `ReplacementKind::RedirectZoneChange { to: Library }`,
  noted into `state.shuffled_instead_of_graveyard` like the exile version; `apply::replace`
  redirects to the owner's library and `apply`/`apply_simultaneous` then shuffle it.
- "Enchanted creature has protection from X. This effect doesn't remove this Aura." —
  `Restriction::Protection { spares_source: true }`; `eval::protected_from` ignores such
  an effect when the object being checked is the effect's own source.
- "Exile ~ with N <kind> counters on it." — `Effect::ExileSelfWithCounters`; on a card with
  suspend that leaves it suspended, so the existing upkeep countdown takes over.

Tests: `shuffle_instead.rs` (destroyed / discarded → owner's library, shuffled),
`aura_protection.rs` (Aura stays and green spells can't target; without the clause it
falls off), `rift.rs` (exiled with three counters, two after the next upkeep).
