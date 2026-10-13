# Card coverage: control transfers, protected mana, combined triggers, repeated processes

Continued from the preceding coverage changes. Playable coverage increased
**17,468 → 17,562 / 34,914 (50.3%)**: 94 additional cards and no losses in a full
catalog comparison. Together with the preceding round, this is 138 additions against
HEAD. The same `/tmp` SQLite backup was used throughout.

New Commander staples: **Wishclaw Talisman, Delighted Halfling, Cavern of Souls,
Derevi, Empyrial Tactician, Orcish Bowmasters, Ad Nauseam**. Other gains include
Up the Beanstalk, Humble Defector, Donate, Harmless Offering, Wrong Turn,
Chart a Course, Assemble the Legion, Domri's Ambush, and Hunt the Weak.

Implemented:

- `Effect::ChoosePlayer` chooses an opponent during resolution without targeting.
  `GainControl` freezes its recipient, independently of the resolving effect's
  controller. Wishclaw completes its tutor before giving the artifact to the chosen
  opponent. Targeted donations and temporary control transfers share this path.
- Spending-benefit provenance follows restricted mana through production, floating,
  and consumption. Eligible mana that prevents countering is preferred over ordinary
  mana of the same color. A spell receives protection only when that mana is actually
  spent; copies clear the benefit. Ineligible mana stays restricted and unspent.
  Manual restricted-mana activation now uses the existing provenance machinery.
  Cavern's chosen creature type is frozen when its mana is produced, so the mana
  remains usable after the land leaves. Mana spent during resolution also carries
  its benefit to the resulting spell.
- Combined `When … and whenever …` heads compile independently and merge only if
  they produce identical effects, target slots, and other trigger properties.
  Combat-only and nth-draw qualifications are checked inside each `AnyOf` arm.
- Explicit command-zone abilities are offered only to their card's owner and function
  only from that zone. Derevi's activation pays its printed ability cost, bypasses
  commander casting tax, does not increment spell/cast counts, and triggers its entry.
- Draw triggers can exclude the first actual draw in the drawing player's own draw
  step. Detection uses the event history, distinguishes multiple draws in one batch,
  and does not confuse an upkeep draw with the draw-step draw. Bowmasters' entry and
  draw triggers deal damage and amass. Amassing Orcs adds Orc to an existing Zombie
  Army while retaining its other creature types.
- Fixed separator precedence: `. then ` must be consumed before `. `, otherwise the
  existing longer spelling is unreachable. This unlocks numerous ordinary spells.
- `Effect::RepeatMay` performs the process once and then asks after each iteration.
  Ad Nauseam reads each newly revealed card's mana value, stops when declined, and
  does not reuse the last value after the library empties. The continuation prompt's
  projected view shows the newly obtained card and updated life before the decision.

Added 21 compiled-card regressions in `wishclaw.rs`, `halfling.rs`, `derevi.rs`,
`bowmasters.rs`, and `ad_nauseam.rs`; updated the independent manual-restricted-mana
test to assert that floating preserves the restriction. The existing 14-test restricted
mana review, including generated capacity checks, passes.

One existing fixture, `issue_local_restricted_mana_leftover_leak.json`, needed its
initial expected priority choice updated because manual restricted activation is now
available. A recursive review confirmed that its only other changes are the same
choice in the undo snapshot and the dependent checkpoint digest. Its final digest and
all gameplay expectations are unchanged. The complete replay/regression corpus passes.

Validation: `cargo test --workspace --quiet`, `cargo clippy --workspace --all-targets
-- -D warnings`, `cargo fmt --all --check`, and `git diff --check` pass. Workspace
tests ran outside the sandbox for the existing loopback networking tests.
