# Visual game-state validation — 2026-10-06

Inspected screenshots from the real native egui arena renderer at approximately
1280×860 and 847×689. Used invented demo cards and deterministic `PlayerView`
fixtures, without reading or modifying saved decks. The fixtures exercise
presentation, not engine state transitions or the legality of the constructed
positions.

| State | Checks |
| --- | --- |
| Three mulligans taken | Keep/mulligan controls and bottoming explanation |
| Bottom three cards | Exact selection requirement, disabled confirmation before selection |
| Nine-card cleanup hand | Discard two, horizontal access to final cards, confirmation |
| Two blockers and trample | 3 + 1 blocker damage, 1 damage to opponent, default/custom controls |
| Two target slots and prevention | Independent repeated card controls, recipient names and combat prevention status |
| Optional modes | Zero through two modes, confirmation reachable |
| Two stack objects | Last stacked object displayed first and named by the priority button |
| Crowded battlefield | 67 permanents across both players, graveyard/exile counts, life 1, poison 9, commander damage 20/21 |
| Closed game channel | Ended-game message and absence of active decision controls |

Found and fixed two classes of UI failure in `crates/mtg-app/src/ui.rs`:

- Card interaction IDs were global per object. Repeating a card on the board and
  across target slots produced visible egui duplicate-ID warnings and ambiguous
  click targets. IDs now use their UI scope, with explicit per-slot option scopes.
  Two regression tests verify separate card click targets and selection of only
  the clicked target slot.
- Nested card-choice rows extended horizontally beyond the window, while long
  questions consumed the space for other game panels. Card selection rows now
  scroll horizontally; decision panels have bounded height and scroll vertically.
  Captures after injected wheel input confirmed that the final cards and
  confirmation buttons are reachable at the narrow size. The battlefield retains
  its existing scroll controls; large positions still require scrolling.

Validation:

```sh
cargo test -p mtg-app -p mtg-engine -p mtg-policy --offline
cargo clippy -p mtg-app --lib --example visual_states --offline -- -D warnings
```

541 tests passed, zero failed, 27 ignored. Loopback network tests required running
outside the sandbox. The final targeted UI suite has 25 passing tests.

Repeat the visual captures with a desktop session:

```sh
cargo run -p mtg-app --example visual_states -- /tmp/mtgo-visual
```

The runner captures nine positions and additional scrolled views, mutes audio,
uses a temporary artwork cache, and closes its own window when finished. A tiling
window manager controls the actual viewport dimensions; resize the fixture window
during its initial five-second delay to check another size.

Reviewed artifacts for this session are in `/tmp/mtgo-visual-final-standard/`,
`/tmp/mtgo-visual-scroll/`, and `/tmp/mtgo-visual-verified/`; test output is in
`/tmp/mtgo-visual-final-tests.log`. These temporary artifacts are not committed.

Concurrent rules/compiler work continued during validation. No engine changes
were made for this task. A temporary missing `AltCost::Pay` display arm was added
to unblock the build; the concurrent work added the same arm, so the duplicate
was removed. Other existing changes were preserved.

This validates the listed UI states and existing automated rules coverage. It
does not establish that every imported card interaction works, and it is not a
complete manual playthrough of a live match.
