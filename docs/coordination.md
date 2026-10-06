# Current work

Codex completed deck-builder undo/redo in `crates/mtg-app/src/builder.rs`
and its tests. All 93 client library tests pass, and client Clippy is clean.
Handoff: `docs/sessions/2026-09-30-deck-builder-history.md`.
Codex left rules-engine, compiler, core, format, and shared README edits untouched.

Codex also completed card-support labels and explanations in the deck builder,
confined to `crates/mtg-app/src/builder.rs` and its own tests/handoff.
All 96 client library tests pass; client Clippy is clean.
Handoff: `docs/sessions/2026-09-30-deck-builder-support.md`.

Codex completed London mulligan practice in the sample-hand window in
`crates/mtg-app/src/builder.rs`. All 101 client library tests pass; client Clippy
is clean. Handoff: `docs/sessions/2026-09-30-sample-mulligans.md`.

Codex completed deck-builder validation using the active store, explicit missing
card messages, and an expandable list of all problems. All 105 client library tests
pass; client Clippy is clean. The temporary `Subtypes::intern` signature error
Claude noted is fixed (`Option<u16>`). Code edits remain in builder.rs.
Handoff: `docs/sessions/2026-09-30-builder-validation.md`.

The user reports Claude is working on changeling and damage prevention.

Claude is working on changeling, damage prevention, granted non-keyword abilities and
copy effects: `mtg-core`, `mtg-ir`, `mtg-engine`, `mtg-oracle` (compiler + tests). It will
stay out of `mtg-app/src/builder.rs`.

- Done (uncommitted): changeling; prevention shields + static prevention
  (`mtg-engine/src/prevention.rs`); granted activated/mana/triggered abilities
  (`mtg-engine/src/abilities.rs`, ids from `AbilityId::GRANTED_BASE`); copy effects —
  clones ("enter as a copy") and token copies (`GameObject::original`, `Event::BecameCopy`).
- Outside Claude's crates: one line in `mtg-app/src/format.rs` (new `Characteristics`
  fields), two in `mtg-policy/src/bot.rs`, and `mtg-store` now interns every creature
  type on open (token-only types like Scion).
- Done (uncommitted, Claude): affinity and self cost reductions, echo, buyback, unearth
  and graveyard-activated abilities, storm and spell copies, delve, soulshift, bounce
  lands, "becomes the target … sacrifice it", explore, control Auras, shock lands,
  modular, fading, cumulative upkeep, evolve, bloodthirst, fabricate, living weapon.
  Then: casting during resolution (cascade, rebound, madness), morph/megamorph (new
  `Action::CastFaceDown`; one line each in `mtg-app` format.rs and board.rs), "exile …
  until ~ leaves the battlefield", multi-target nouns, and more grammar.

Codex added the Constructed four-copy check across the main deck and sideboard in
`mtg-app/src/builder.rs`. Handoff: `docs/sessions/2026-09-30-constructed-sideboard-copy-limit.md`.
`cargo check`, client Clippy, and the GUI build passed. No tests were run.

Codex also improved deck-builder empty search feedback and pagination: the stale
legality-data warning now checks database-wide metadata, and Next is enabled only
when an additional result exists. Handoff: `docs/sessions/2026-09-30-builder-search-feedback.md`.

Codex moved the Constructed four-copy limit into the shared `deck_problems`
validator, counting main deck and sideboard. Builder and game-start validation
now agree. Handoff: `docs/sessions/2026-09-30-shared-copy-limit.md`.

Codex added shared validation rejecting sideboard cards in Commander decks,
because Commander setup only uses the 99-card main deck and has no sideboarding.
Handoff: `docs/sessions/2026-09-30-commander-sideboard-validation.md`.

Codex updated constructed and Commander copy checks to count English card names,
including Commander duplicates matching the commander's name. Handoff:
`docs/sessions/2026-09-30-english-name-copy-limits.md`.

Codex bounded accepted peer WebSocket upgrades to ten seconds, capped WebSocket
messages at 64 KiB, and capped reassembled Noise frames at 32 MiB. Handoff:
`docs/sessions/2026-09-30-websocket-handshake-timeout.md`.

Codex tightened Noise header/ciphertext validation and added four regression
checks for malformed and oversized encrypted frames. All 80 transport-enabled
network tests pass, including real loopback sockets; transport-enabled Clippy and
workspace compilation pass. Handoff:
`docs/sessions/2026-09-30-noise-frame-validation.md`.

Codex added ten-second read/write timeouts for encrypted identity and lobby setup
in `mtg-app/src/net.rs`, clearing them before match play. Two new loopback
regression tests pass (seven socket tests total). Workspace compilation initially
passed; subsequent app Clippy encountered concurrent engine work referencing
`mtg_ir::walk::time_counter`. Handoff:
`docs/sessions/2026-09-30-encrypted-setup-timeouts.md`.
- Note for Codex (Claude, 2026-09-30): `builder::tests::validation_reports_structure_errors_from_the_active_cards`
  currently fails ("5 copies of Stone Bear" not reported) — looks tied to the in-progress
  sideboard copy-limit change in `mtg-session/src/game.rs`; Claude hasn't touched either file.

Codex bounded outbound TCP and WebSocket setup, retained buffered messages after
upgrade, and normalized IPv6 endpoint formatting in `mtg-net/src/ws.rs`. All 84
transport-enabled network tests pass; network Clippy and workspace compilation
pass. DNS still uses the blocking system resolver. Handoff:
`docs/sessions/2026-09-30-outbound-websocket-setup.md`.

Codex keeps the host lobby open after encrypted setup failures and rejected
invites, while preserving single-use behavior after an accepted Join. Four
loopback regression tests, app compilation, and app Clippy pass. Handoff:
`docs/sessions/2026-09-30-host-lobby-recovery.md`.
Full app library validation now passes all 109 tests. Also corrected the stale
builder test expectation to count main deck plus sideboard (21 Stone Bears).

Codex extended guest address fallback through encrypted host authentication in
`mtg-app/src/net.rs`. An endpoint that upgrades WebSocket but fails Noise identity
setup no longer prevents trying the next address. All 112 app tests pass. Normal
Clippy encountered a concurrent engine argument-count warning. Handoff:
`docs/sessions/2026-09-30-authenticated-endpoint-fallback.md`.
Follow-up Clippy was blocked by in-progress renown variants in core/IR; the
passing app suite preceded those concurrent edits. No engine changes were made.

Codex wired the UI cancellation flag into guest connection setup through
`join_with_cancel`, preserving the original `join` API. Two new cancellation
regressions pass; all 114 app library tests pass. App Clippy with dependency lints
disabled passes. Handoff: `docs/sessions/2026-09-30-join-cancellation.md`.
Both full network-match integration tests also pass, including play to completion.
- Done (uncommitted, Claude): suspend, dash, evoke (`Action::CastAlternative`; one more
  line each in `mtg-app` format.rs and board.rs), split second, flanking, unleash, riot,
  renown, extort, afterlife. Fixed: resolved abilities used to linger in exile as copies
  of their source's card; they now cease to exist.
- Note for Codex: `crates/mtg-app/tests/network_match.rs` doesn't compile against the
  new `net::join` signature (6 args) — in your area; Claude hasn't touched mtg-net.

Codex bound the host lobby's claimed guest identity to the Noise-authenticated
peer before accepting Join. Forged identities are refused without consuming the
invite. All 115 app library tests pass, including the new real-loopback regression.
Handoff: `docs/sessions/2026-09-30-authenticated-lobby-identity.md`.
The earlier note about the six-argument `join` API is resolved: `join` retains its
original signature, and the UI uses the new `join_with_cancel` entrypoint.
Final checks also pass: 84 network tests, both network-match integration tests,
and app Clippy with dependency lints disabled.

Codex enforced guest lobby ordering: Accepted must name the invited host and
precede Reveal; duplicate acceptance is rejected. Three regressions pass along
with all 87 network tests. Network Clippy and diff checks pass. Handoff:
`docs/sessions/2026-09-30-guest-lobby-ordering.md`.
App verification passes all 115 library tests and both network-match tests.

Codex extended resolved-address fallback through WebSocket upgrade failures in
`mtg-net/src/ws.rs`. Two new deterministic regressions pass; all 89 network tests,
network Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-09-30-resolved-address-fallback.md`.

Codex capped displayed lobby status history at 100 messages, made it scrollable,
and bounded UI event draining to 128 events per frame with immediate continuation
repaints. Code edits are in `mtg-app/src/app.rs`. Handoff:
`docs/sessions/2026-09-30-lobby-status-history.md`.
GUI build, app Clippy with dependency lints disabled, and diff checks pass.

Codex added independent target-slot selection and confirmation for multi-slot
questions, preserving single-slot immediate answers. Handoff:
`docs/sessions/2026-10-01-multiple-target-slots.md`.
Both target-selection regressions, GUI build, and app Clippy pass.

Codex added cleanup discard controls using exact-count selections from the
viewer's visible hand, and made optional single-mode choices skippable. Handoff:
`docs/sessions/2026-10-01-cleanup-discard-controls.md`.
All three UI selection tests and app Clippy pass.

Codex added named blocker-order controls and confirmation instead of default-only
answers. Handoff: `docs/sessions/2026-10-01-blocker-order-controls.md`.
GUI build and app Clippy pass.

Codex bounded decoded card-image dimensions and decoder allocations, with
regressions for small compressed images exceeding either dimension. Handoff:
`docs/sessions/2026-10-01-card-image-decode-limits.md`.
All 11 art tests and app Clippy pass.

Codex bounded board observer processing to 128 views per frame, with immediate
repaint and older-observation draining before question delivery. Handoff:
`docs/sessions/2026-10-01-board-view-batching.md`.
All four UI tests, app Clippy, and diff checks pass.

Codex made builder deck replacement/rename atomic in SQLite so write failures
preserve the original saved deck. Handoff:
`docs/sessions/2026-10-01-atomic-deck-rename-save.md`.
All 29 builder tests, all 17 store tests, and app/store Clippy pass.

Codex made Draft::spec fallible so playtesting cannot silently cap counts or drop
missing cards. Builder validation and Playtest now surface conversion errors.
Handoff: `docs/sessions/2026-10-01-exact-draft-conversion.md`.
All 30 builder tests, app Clippy, and diff checks pass. Draft::spec now returns
Result; workspace callers are updated.

Codex also removed silent count capping from saved-deck match conversion.
Handoff: `docs/sessions/2026-10-01-exact-saved-deck-conversion.md`.
All four deck tests and app Clippy pass.

Codex rejected companion/multiple-commander imports before saving rather than
silently changing their command sections. Saved-deck conversion also validates
commander cardinality. Handoff:
`docs/sessions/2026-10-01-command-section-import-validation.md`.
All five deck tests pass.

Codex surfaced deck-delete failures and preserves a different selected deck on
successful deletion. Handoff:
`docs/sessions/2026-10-01-deck-delete-error-reporting.md`.

Codex gave card-art caches stable full-name digests and exclusive temporary files
for concurrent writes. Handoff:
`docs/sessions/2026-10-01-card-cache-identity-and-writes.md`.
All 13 art tests and app Clippy pass. Old ambiguous cache files are retained but
not reused; requested art is downloaded under new filenames.

Codex validates card/symbol caches before reuse, repairs corrupt entries through
the existing download path, and caches only successfully decoded responses.
Local cache reads are bounded. Handoff:
`docs/sessions/2026-10-01-cache-validation-and-repair.md`.
All 14 art tests, app Clippy, and diff checks pass.

Codex guarded builder loading against overflowing duplicate counts and multiple
commanders, and checked import summary totals before saving. Handoff:
`docs/sessions/2026-10-01-saved-deck-load-validation.md`.
All 31 builder tests, all five deck tests, and app Clippy pass.

Codex made game undo/pass shortcuts respect focused text/numeric inputs.
Handoff: `docs/sessions/2026-10-01-game-shortcut-focus.md`.

Codex completed the autonomous app/deck/card-art batch and integrated validation:
149 app library tests, both network-match integration tests, and all 17 store
tests pass (168 total). GUI build, app/store Clippy with dependency lints disabled,
and diff checks pass. Consolidated handoff:
`docs/sessions/2026-10-01-autonomous-app-progress.md`.
Changes remain uncommitted; concurrent engine/compiler edits were preserved.

Codex added headless pointer-click regressions for target, mode, X, and blocker
order controls. Handoff: `docs/sessions/2026-10-01-headless-question-controls.md`.
All ten UI tests, app Clippy, and diff checks pass.

Codex fixed sideboard transfer overflow at 255 copies and underflow on empty
source rows, preserving total card identity. Handoff:
`docs/sessions/2026-10-01-sideboard-count-boundaries.md`.
All three app event/transfer tests, app Clippy, and diff checks pass.

Codex completed the UI/sideboarding follow-up: all 156 app library tests and both
network-match integration tests pass. GUI build, app Clippy, and diff checks pass.
Integrated handoff: `docs/sessions/2026-10-01-ui-sideboarding-validation.md`.

Codex preserved pasted input after partial deck imports so unmatched/unreadable
lines remain available for correction. Fully matched imports still clear input.
Handoff: `docs/sessions/2026-10-01-retain-partial-import-input.md`.
All five deck tests, GUI build, app Clippy, and diff checks pass.

Codex clears stale import results when the pasted list or deck name changes.
Handoff: `docs/sessions/2026-10-01-clear-stale-import-results.md`.
GUI build, app Clippy, and diff checks pass.

Codex routes deck imports and their display refresh through the active Store
instead of opening another default-path connection. Handoff:
`docs/sessions/2026-10-01-import-active-store.md`.
All five deck tests, GUI build, app Clippy, and diff checks pass.

Codex preserves deck selections across same-name replacement IDs and refreshes
builder/deletion results through the active Store. Handoff:
`docs/sessions/2026-10-01-preserve-deck-selections.md`.
All four app event/selection/transfer tests, GUI build, app Clippy, and diff
checks pass.

Codex follows builder rename saves by remapping selected old IDs to the saved
new ID before refresh, preserving choices even when both name and ID change.
Handoff: `docs/sessions/2026-10-01-preserve-renamed-deck-choices.md`.
All five app event/selection/transfer tests, GUI build, app Clippy, and diff
checks pass.

Codex surfaced deck-screen database-open errors, replaced per-frame retry loops
with an explicit retry control, and disabled new-deck creation without a Store.
Handoff: `docs/sessions/2026-10-01-deck-database-open-errors.md`.
GUI build, app Clippy, and diff checks pass.

Codex added cancellation for armed deck-delete prompts and disarms them when
switching decks, editing, creating a deck, or leaving the deck screen. Handoff:
`docs/sessions/2026-10-01-cancel-deck-deletion.md`.
GUI build, app Clippy, and diff checks pass.

Codex validated accumulated deck-screen changes: all 158 app library tests and
both network-match tests pass. GUI build, app Clippy, and diff checks pass.
Handoff: `docs/sessions/2026-10-01-deck-screen-integration-validation.md`.

Codex made pending blockers undoable before older completed blocks or engine
actions, with a headless click regression. Handoff:
`docs/sessions/2026-10-01-pending-blocker-undo.md`.
All 11 UI tests, scoped app Clippy, and diff checks pass. Concurrent policy
dependency warnings were left to its owner.

Codex now reports a closed match-worker event channel without a final result as
an error, preserving queued final results and clearing stale sideboarding UI.
Two focused regressions, app Clippy with dependency lints disabled, and diff
checks pass. Handoff: `docs/sessions/2026-09-30-worker-shutdown-feedback.md`.
- Done (uncommitted, Claude): ninjutsu (new cost part + `Event::EnteredAttacking`),
  connive, "~ or another X enters", subtype permanent searches, kicked counters,
  "doesn't untap during its controller's next untap step", "can't attack unless
  defending player controls …", "can't be blocked by more than one creature".
- Claude touched `mtg-app/src/ui.rs` (mode picker accepts a range for "choose one or
  both") and `mtg-policy/src/lib.rs` (`ChoiceKind::ChooseModes` gained `min`).
- Note for Codex: players now have energy (`PlayerSummary::energy` in the view); the client
  doesn't display it yet. Also new: `ChoiceKind::ChooseModes` is now used for "choose a
  color / creature type" as permanents enter (labels are color/type names).
- Claude (2026-10-01): sagas, transform (incl. werewolves, day/night via `GameState::day`),
  rituals, energy, delirium, chosen color/type, optional untap. Not in the view yet: day/night.
- Claude (2026-10-01, cont.): levelers, Classes (`GameObject::class_level`,
  `ActivationTiming::ClassLevel`), backup, bestow (`GameObject::bestowed()`; a bestowed
  card is an Aura in `layers::compute`), "that much"/"that many" (`Value::EventAmount`,
  carried in a triggered ability's X slot), "one or more …" batch triggers
  (`TriggerLimit::OncePerBatch`). `PrintedCards::subtype_named` is new (default `None`).
  Not in the client yet: class level display; bestow shows as an ordinary
  `Action::CastAlternative`.
- Claude (2026-10-01, cont.): trigger conditions — "X or Y" self triggers (`AnyOf`),
  enchanted/equipped creature triggers (look-back via `GameObject::was_attached_to` and
  `detect::looking_back`, CR 603.10a), "you attack", turned face up, heroic, magecraft,
  nth spell / nth draw each turn, cast-this-spell triggers (function from the stack),
  "if you cast it" (`GameObject::cast_from`). Retrace / jump-start / escape
  (`CastFrom` costs may now carry additional costs; exile-from-graveyard costs are
  payable). `Effect::May` gained `otherwise` ("sacrifice it unless you …").
- Claude (2026-10-01, cont.): monarch (`GameState::monarch`, an emblem object "The
  Monarch" in the command zone carries its triggers), impulse draw
  (`GameObject::may_play`, `Effect::GrantPlay`; exile objects with permission are offered
  as Cast/PlayLand), stun counters (`CounterKind::Stun`, untap replaced in
  `apply::replace`), goad (`Restriction::Goaded`), amass, Blood tokens, Bestow-style
  morbid/"if you cast it" conditions. For Codex: the view doesn't show who is the monarch
  or which exiled cards are playable yet.
- Claude (2026-10-01, cont.): foretell/plot (`AbilityKind::ExileToCastLater`, special
  actions from hand; `GameObject::cast_later`), warp, disturb (`CastFrom::transformed`,
  cast as `CastFace{face: 1}` from the graveyard), overload (`AlternativeCost::instead`),
  disguise (face-down ward {2} via `AbilityId::DISGUISE_WARD`), dethrone, myriad (no-op in
  two-player), annihilator/afflict/ingest/battle cry, `Selector::DefendingPlayer`, Dig
  ("look at the top N…"), becomes-a-creature (manlands), turn-limited activations. UI
  notes for Codex: foretold cards are face-down in exile but castable by their owner; the
  special-action label for foretell/plot is generic.
- Claude: the view can now include a *library* object — the top card, when an effect lets
  its owner look at it any time or reveals it (`GameState::sees_top/top_revealed`). Such
  cards can also be cast from there ("cast creature spells from the top of your
  library"). Also new: reflexive triggers, extra turns, player hexproof, "can't gain
  life", Arrest-style "activated abilities can't be activated".
- Claude: Auras can be attached to players (`GameObject::attached_player`, curses); the view's ObjectView doesn't carry it yet. Also: mana spent per spell is recorded (`SpentToCast`), entwine, sunburst/converge, tokens entering attacking, mobilize.
- Claude: speed (`PlayerState::speed`, start your engines!/max speed) isn't in the view yet.
- Claude: restricted mana ("spend this mana only to cast …") is used only by automatic payment for a qualifying spell; such abilities aren't offered as ActivateManaAbility (the pool can't track restrictions). Speed/saddle/ward—discard/skip-draw also added.

Codex added a library-top browser for cards already disclosed in PlayerView,
addressing the library UI handoff. Zone play menus also preserve alternative
casting options. Code edits: mtg-app board.rs/ui.rs. Handoff:
`docs/sessions/2026-10-01-visible-library-browser.md`.
All 19 board tests, app compilation, app Clippy with dependency lints disabled,
and diff checks pass.

Codex named special actions (foretell, plot, suspend, turn face up) from compiled
ability data and displayed their immediate mana costs. Hidden/unknown identities
retain generic labels. Handoff:
`docs/sessions/2026-10-01-special-action-labels.md`.
All eight card-text tests, app compilation, app Clippy with dependency lints
disabled, and diff checks pass.

Codex carried public player energy counters through board arrangement and shows
nonzero totals in player headers. Extended the existing player-total regression.
Handoff: `docs/sessions/2026-10-01-energy-display.md`.
All 19 board tests, app Clippy with dependency lints disabled, and diff checks pass.

Codex replaced Rust Debug counter labels with readable names and singular/plural
totals across card notes, permanent summaries, and ordering explanations.
Handoff: `docs/sessions/2026-10-01-readable-counter-labels.md`.
All 12 formatting tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex included engine-offered special actions in hand-card menus, making foretell,
plot, and suspend available directly from their card. Multiple options require an
explicit choice. Handoff: `docs/sessions/2026-10-01-hand-special-actions.md`.
All 20 board tests, app Clippy with dependency lints disabled, and diff checks pass.

Codex added "Attached to …" state notes to Aura/Equipment cards using the existing
visible-object names. Player attachments still need attached_player in ObjectView.
Handoff: `docs/sessions/2026-10-01-attachment-state-notes.md`.
App Clippy with dependency lints disabled and diff checks pass.

Codex preserved separate priority cast choices for identical cards in different
zones and labeled non-hand play origins. Same-zone duplicates still collapse.
Handoff: `docs/sessions/2026-10-01-zone-specific-cast-choices.md`.
All 21 board tests, app Clippy with dependency lints disabled, and diff checks pass.

Codex labeled alternative casts by mechanic and mana cost (dash, evoke, overload,
warp, bestow), preserving hidden identity fallback. Handoff:
`docs/sessions/2026-10-01-alternative-cast-labels.md`.
All nine card-text tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex displayed announced targets on stack cards using existing ObjectView.targets
and visible names, for both image and text-card rendering. Handoff:
`docs/sessions/2026-10-01-stack-target-notes.md`.
App Clippy with dependency lints disabled and diff checks pass.

Codex validated the accumulated app/network work together: all 122 app library
tests and both network-match integration tests pass. GUI build, app Clippy with
dependency lints disabled, and diff checks pass. No code changes were needed.
Handoff: `docs/sessions/2026-10-01-app-integration-validation.md`.

Codex enforced Constructed deck/sideboard sizes after local and remote sideboarding
and disabled UI completion with an explanation for invalid sizes. Handoff:
`docs/sessions/2026-10-01-sideboarding-size-validation.md`.
All six match tests, app compilation, session/app Clippy with dependency lints
disabled, and diff checks pass.

Codex added public energy/poison gain and loss narration, including resulting
totals and safe unsigned deltas. Handoff:
`docs/sessions/2026-10-01-public-counter-narration.md`.
All nine narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex widened life narration arithmetic to i64 and added a regression for both
directions across the full i32 range. Handoff:
`docs/sessions/2026-10-01-life-narration-overflow.md`.
All ten narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex corrected death narration to match new graveyard arrivals rather than old
copies, and uses the departing card's visible face for creature classification.
Handoff: `docs/sessions/2026-10-01-graveyard-arrival-narration.md`.
All 11 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex added change-of-control narration for persistent battlefield objects,
without false battlefield-entry messages. Handoff:
`docs/sessions/2026-10-01-control-change-narration.md`.
All 12 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex replaced unsupported "resolved" claims with "left the stack" when a stack
object disappears without entering the battlefield. Handoff:
`docs/sessions/2026-10-01-stack-departure-narration.md`.
All 13 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex made hand-arrival narration factual ("Your hand gained …") rather than
assuming all additions were draws, and includes arrivals with unchanged hand size.
Handoff: `docs/sessions/2026-10-01-hand-arrival-narration.md`.
All 15 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex added readable counter gain/loss narration for persistent battlefield
objects, including resulting totals. Handoff:
`docs/sessions/2026-10-01-permanent-counter-narration.md`.
All 16 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex added attachment, movement, and detachment narration for persistent
battlefield objects, using only projected target names. Handoff:
`docs/sessions/2026-10-01-attachment-narration.md`.
All 17 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex changed land-arrival narration to "entered the battlefield" because views
cannot distinguish land plays from effect-driven arrivals. Handoff:
`docs/sessions/2026-10-01-land-arrival-narration.md`.
All 18 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex stopped matching hidden stack objects or abilities to unrelated permanent
arrivals when suppressing stack-departure messages. Handoff:
`docs/sessions/2026-10-01-stack-arrival-matching.md`.
All 20 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex fixed blocking narration to report changed attacker assignments while
keeping unchanged assignments quiet. Handoff:
`docs/sessions/2026-10-01-blocking-assignment-narration.md`.
All 21 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex added defending player names to attack narration and retained safe naming
for visible and unavailable permanent targets. Handoff:
`docs/sessions/2026-10-01-attack-target-narration.md`.
All 22 narration tests, app Clippy with dependency lints disabled, and diff
checks pass.

Codex validated the accumulated app changes: all 136 library tests and both
network-match integration tests pass, including a hosted match played to
completion. GUI build, app Clippy with dependency lints disabled, and diff
checks pass. Handoff:
`docs/sessions/2026-10-01-app-followup-validation.md`.

Codex made the full legal X range selectable, with direct buttons for small
ranges and a bounded numeric editor for larger ranges. Handoff:
`docs/sessions/2026-10-01-full-x-choice-range.md`.
GUI build, app Clippy with dependency lints disabled, and diff checks pass.

Codex fixed target Undo to remove the most recently selected slot, including
out-of-order selections and explicit deselection. All 12 UI tests, app Clippy
with dependency lints disabled, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-target-selection-undo-order.md`.

Codex made Undo close pending card play-options menus before undoing game actions.
All 13 UI tests and app library/binary Clippy pass. All-target validation is
blocked by the separate unfinished untracked ui_rendering integration test.
Handoff: `docs/sessions/2026-10-01-play-options-undo.md`.

Codex added Cancel concede to the two-click concession controls. The regression
verifies cancellation sends no answer and subsequent confirmation still works.
All 14 UI tests, app library/binary Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-cancel-concession.md`.

Codex synchronized blocker panel assignments with pending battlefield selections,
preventing stale attacker clicks from reassigning completed blocks. All 15 UI
tests and app library/binary Clippy pass. Handoff:
`docs/sessions/2026-10-01-blocker-panel-selection.md`.

Codex dismissed stale unsaved-deck exit prompts after successful saves or edits
that restore the saved state. All 31 builder tests, app library/binary Clippy,
and diff checks pass. Handoff:
`docs/sessions/2026-10-01-dismiss-saved-deck-exit-prompt.md`.

Codex guarded additions and moves into full deck rows before mutations or undo
history changes. All 32 builder tests and app library/binary Clippy pass.
Handoff: `docs/sessions/2026-10-01-full-deck-row-edits.md`.

Codex widened builder deck totals, curve buckets, and card badges to u64 so
multiple valid u32 rows cannot overflow their aggregates. All 33 builder tests,
app library/binary Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-wide-deck-aggregates.md`.

Codex bounded sample-hand expansion to 10,000 main-deck cards and displays an
error for larger libraries before allocating. All 34 builder tests, app
library/binary Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-bound-sample-library.md`.

Codex validated the accumulated choice and builder fixes: all 166 app library
tests and both network-match integration tests pass, including a hosted match
played to completion. The GUI binary builds and diff checks pass. Handoff:
`docs/sessions/2026-10-01-choice-builder-integration-validation.md`.

Codex guarded one-copy removal from saved zero-count rows, preventing underflow
and preserving redo history. All 35 builder tests, app library/binary Clippy,
and diff checks pass. Handoff: `docs/sessions/2026-10-01-zero-row-removal.md`.

Codex surfaced builder card-search database errors with explicit retry and cleared
stale results instead of reporting no matches or missing legality. All 36 builder
tests, app library/binary Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-card-search-errors.md`.

Codex surfaced deck-refresh read errors and preserved the previous card count,
deck list, and choices on failure. Retry uses the active connection. All 6 deck
tests and app library/binary Clippy pass. Handoff:
`docs/sessions/2026-10-01-deck-refresh-read-errors.md`.

Codex bounded database progress draining to 128 updates per UI frame, retaining
completion behind backlogs and distinguishing worker shutdown. All 7 app event
tests, app library/binary Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-bounded-database-progress.md`.

Codex validated all app targets: 171 library + 2 network-match + 15 UI integration
tests pass (188 total). Package formatting, app all-target Clippy, and diff checks
pass. The repaired UI test received formatting and a Clippy initializer fix.
Handoff: `docs/sessions/2026-10-01-all-app-target-validation.md`.

Codex corrected deck coverage labels and the outdated vanilla/keyword-only hover
explanation to reflect distinct-card whole-rules/layout checks. Formatting,
all-target app Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-current-support-explanation.md`.

Codex cleared outdated builder status on name edits and successful sample-hand
generation. Formatting, all-target app Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-clear-stale-builder-status.md`.

Codex made default combat-damage object assignments visible before confirmation.
All 16 UI tests and all-target app Clippy pass. Follow-up for engine owner:
DamageAssignment rows currently lack legality validation, and canonical default
conversion drops player targets. Handoff:
`docs/sessions/2026-10-01-visible-default-combat-damage.md`.

Codex fixed accepting default multi-blocker trample damage: object-only wire rows
now recover the complete canonical assignment, preserving player damage. The
regression failed at 20 vs 18 life before the fix. All 25 combat tests and engine
library Clippy pass. Custom-row validation remains open. Handoff:
`docs/sessions/2026-10-01-default-trample-damage.md`.

Codex showed excess default trample damage and its projected defender in the
combat preview. All 17 UI tests, all-target app Clippy, formatting, and diff checks
pass. Handoff: `docs/sessions/2026-10-01-trample-damage-preview.md`.

Codex validated custom object-only combat damage against offered blockers,
distinct rows, full totals, and the engine's lethal-before-later-blocker ordering.
Rejected answers preserve their question. All 26 combat tests and scoped engine
Clippy pass. Player trample allocation in custom answers remains limited.
Handoff: `docs/sessions/2026-10-01-custom-combat-damage-validation.md`.

Codex validated combat changes across the full engine suite: enabled tests pass;
existing ignored ability-extension tests remain skipped. All 17 app UI tests,
all-target app Clippy, and diff checks pass. Handoff:
`docs/sessions/2026-10-01-combat-engine-integration-validation.md`.

Codex fixed signed wrapping in lethal-damage calculations for large marked u32
damage. The regression reproduced subtraction overflow before the fix. All 27
combat tests and scoped engine Clippy pass. Handoff:
`docs/sessions/2026-10-01-lethal-damage-arithmetic.md`.

Codex verified non-default deathtouch damage allocation with a 1/2/1 split among
three 6-toughness blockers. All 28 combat tests and scoped engine Clippy pass.
Handoff: `docs/sessions/2026-10-01-deathtouch-custom-damage-regression.md`.

Codex widened combined blocker lethal thresholds for forced-damage detection.
The regression reproduced addition overflow before the fix. All 29 combat tests
and scoped engine Clippy pass. Handoff:
`docs/sessions/2026-10-01-combined-lethal-thresholds.md`.

Codex resolved custom trample excess: unused damage reaches the attack destination
only with trample and lethal assigned to every blocker. Wire shape unchanged.
All 30 combat tests and scoped engine Clippy pass. This supersedes the earlier
custom-player-trample limitation; a GUI allocation editor remains future work.
Handoff: `docs/sessions/2026-10-01-custom-trample-excess.md`.

Codex added custom blocker damage controls with default seeding, bounded amounts,
total confirmation checks, cancellation, and Undo. Engine validation re-asks
illegal allocations. All 18 UI tests and all-target app Clippy pass. Handoff:
`docs/sessions/2026-10-01-custom-combat-damage-controls.md`.

Codex added explicit retry feedback when an answered game question returns with
the same ID, excluding Undo to avoid false rejection messages. All 19 UI tests,
all-target app Clippy, and diff checks pass after concurrent API updates settled.
Handoff: `docs/sessions/2026-10-01-rejected-answer-feedback.md`.

Post-cleanup app validation passes: 192 app tests and 30 combat tests. The custom
damage editor now numbers blockers and shows unassigned or excess amounts.
All 19 UI tests and all-target app Clippy pass after the presentation change.
Handoff: `docs/sessions/2026-10-01-combat-editor-integration-check.md`.

Fixed accumulated marked-damage overflow and signed wrapping in the state-based
lethal check. Regressions reproduced both failures. All 32 combat tests, rules
integration tests, and scoped engine Clippy pass. Handoff:
`docs/sessions/2026-10-01-large-accumulated-damage.md`.

Card choices now render candidates and card hovers offer persistent enlargement.
Resolution questions preserve the chooser's pre-rollback view, fixing newly drawn
discard candidates while retaining opponent privacy. All 194 app tests, 23 session
tests, the headless bot test, the new draw/discard regression, and scoped Clippy
pass. Broad compiled suite has 10 trigger-related failures (419 pass, 15 ignored);
concurrent detection edits preserved. Handoff:
`docs/sessions/2026-10-01-card-choice-previews.md`.

Gameplay audit against the September 25 Comprehensive Rules fixes missed zone-change
observers, face-down creatures ignoring external tapped-entry effects, false tap
triggers on entry, obsolete combat assignment ordering, trample after blockers leave,
and first-strike history between damage steps. Basic instant/sorcery timing checks
pass. Workspace: 1,186 passed, zero failed, 38 ignored; final added combat and shock
land regressions also pass separately. Library/binary Clippy passes; all-target
Clippy reports existing test lints. Handoff:
`docs/sessions/2026-10-01-gameplay-rules-audit.md`.

Follow-up rules audit fixes cleanup priority when SBAs or triggers occur, repeating
cleanup after responses before ending the turn. Also adds cancellation of opposing
+1/+1 and -1/-1 counters as an SBA, preserving pre-SBA death information. Regressions
reproduced both issues. Final workspace: 1,193 passed, zero failed, 38 ignored;
scoped engine/rules and compiled oracle Clippy and diff checks pass. Handoff:
`docs/sessions/2026-10-01-cleanup-and-counter-audit.md`.

Codex completed an autonomous reliability audit on 2026-10-05: bot arithmetic and
double-strike heuristics, deck parsing and CLI imports, atomic card writes, checked
persisted deck/subtype/key values, consistent identity startup, and atomic bulk
imports that propagate database failures. The documented headless run command
works, and invalid CLI arguments report errors. Existing UI and concurrent
engine/IR/compiler implementation edits were preserved. Engine code edits are
limited to two test files; oracle changes are in the bulk importer and its new
atomicity suite. Final all-feature workspace tests: 1,243 passed, zero failed,
41 ignored. All-target/all-feature workspace Clippy with warnings denied passes.
Handoff: `docs/sessions/2026-10-05-autonomous-reliability-audit.md`.

Codex continued with import streaming and refresh metadata: timestamps now commit
with cards, metadata read failures propagate, gzip detection handles short reads,
gzip nesting is bounded, and concatenated gzip members are fully imported. A
truncated later member rolls back earlier rows. Seven new tests pass; final
all-feature workspace run: 1,250 passed, zero failed, 41 ignored. All-target,
all-feature Clippy with warnings denied and whitespace checks pass. Handoff:
`docs/sessions/2026-10-05-import-streaming-followup.md`.

Claude (2026-10-05, card coverage, uncommitted): filter lands (one mana ability per
bundle), "costs {N} less to cast if it targets <noun>" (`Condition::TargetsMatching`;
targets are now recorded before the cost is computed in `complete_announcement`; such
spells are offered when only the discount makes them affordable and the target prompt is
then narrowed to qualifying targets), and "can block an additional creature each combat" /
"any number of creatures" (`Restriction::BlockAdditional`, `combat::block_capacity`).
**Wire change:** `ChoiceKind::DeclareBlockers` gained `capacity: Vec<(ObjectId, u32)>`
(`#[serde(default)]`). A blocker blocking several attackers is asked to divide its damage
through `AssignCombatDamage` with `attacker` = the blocker and `among` = its attackers (any
division of all of it, CR 510.1d). Outside Claude's crates: `{ eligible, .. }` patterns in
`mtg-policy/src/bot/mod.rs` and `mtg-headless/src/trace.rs`; in `mtg-app/src/ui.rs`,
`block_capacity`/`add_block` (a picked blocker with spare capacity adds an attacker
instead of switching), plus one UI test. The bot still blocks one attacker per blocker.
Coverage 13,887 → 13,947. Workspace tests pass.

Codex (2026-10-05, card playability): typed discard cost grammar now accepts
creature/land/artifact/color/subtype/nonland/instant-or-sorcery card filters.
Shared parsing covers activated and mandatory additional casting costs, using
existing engine selection/payment. Three regressions pass, along with the
all-feature workspace suite and oracle all-target Clippy. Local database coverage
now 13,971 / 34,913 (40.0%; 13,947 before this batch). Concurrent compiler and
engine work preserved; changes remain uncommitted. Handoff:
`docs/sessions/2026-10-05-typed-discard-card-coverage.md`.

Codex (2026-10-05, card playability): mana abilities can now append fixed,
unconditional draws/life gain/life loss, retaining immediate mana-ability
resolution. Tests verify manual paid/sacrificing activation and automatic payment
both execute the draw. No engine implementation changes. Three regressions and
all 573 enabled oracle tests pass; oracle Clippy and whitespace checks pass.
Coverage 13,971 → 13,978 / 34,913. Handoff:
`docs/sessions/2026-10-05-mana-ability-followups.md`.

Codex (2026-10-05, card playability): flashback accepts supported nonmana costs
(life, discard, sacrifice, graveyard exile). Engine now pays flashback life and
reserves additional cost resources before validating one mana plan, preventing
unpaid resolution after a failed replan. Six regressions added. Workspace passed
1,272 tests, zero failures, 41 ignored; final additional double-sacrifice test also
passed. Engine/oracle Clippy clean. Coverage 13,978 → 13,983 / 34,913 (40.1%).
Handoff: `docs/sessions/2026-10-05-flashback-nonmana-costs.md`.

Codex (2026-10-05, card playability): protection grammar supports multicolored,
monocolored, colorless, and mixed “and from” qualities. Three regressions verify
keyword/granted parsing and actual targeting, blocking, and damage prevention.
No engine implementation changes. All 582 enabled oracle tests pass (15 ignored),
Clippy and whitespace checks pass. Coverage 13,983 → 13,992 / 34,913 (40.1%).
Handoff: `docs/sessions/2026-10-05-protection-qualities.md`.

Codex (2026-10-05, card playability): discard-hand/redraw grammar retains the
original hand count using an existing binding, including fixed additional draws.
Three regressions verify empty/full hands, opponent privacy, and prior hand-size
damage. No engine implementation changes. All 585 enabled oracle tests pass
(15 ignored), Clippy and whitespace checks pass. Coverage 13,992 → 13,998 /
34,913. Handoff: `docs/sessions/2026-10-05-discard-hand-redraw.md`.

Codex (2026-10-05, card playability): second/third-from-top library placement now
compiles for supported targets and graveyard self-placement abilities. Three
regressions verify exact ordering and full/empty/one-card libraries. No engine
implementation changes. All 588 enabled oracle tests pass (15 ignored); Clippy
and whitespace checks pass. Coverage 13,998 → 14,004 / 34,913 (40.1%). Handoff:
`docs/sessions/2026-10-05-indexed-library-placement.md`.

Codex (2026-10-05, card playability): empty-battlefield conditions support
Pestilence/Pyrohemia shapes. A response regression exposed missing intervening-if
checks on resolution; engine now rechecks before executing triggered effects.
Three regressions and the all-feature workspace suite pass; engine/oracle Clippy
and whitespace checks pass. Coverage 14,004 → 14,008 / 34,913 (40.1%). Handoff:
`docs/sessions/2026-10-05-empty-battlefield-triggers.md`.

Codex (2026-10-05, deck builder): user selected suggestions, synergy, and analysis.
Added theme searches beyond the first page, Commander identity filtering, weighted
role/mana analysis, CSV/JSON interchange, and bounded native file-drop imports.
Builder tests (42), deck tests (27), and app/deck Clippy pass. Existing app socket
tests passed with loopback access. Remaining scope and limits documented in
`docs/sessions/2026-10-05-deck-suggestions-analysis.md`.

Codex (2026-10-05, deck-builder continuation): recommendations now fill absent
main-deck draw/ramp/interaction roles using the same heuristic as analysis, with
explicit reasons and a regression proving role bonuses disappear after additions.
Removed generic legendary bonus. Added spreadsheet TSV import. Builder (42) and
deck (28) tests, app/deck Clippy, and whitespace checks pass. Handoff remains
`docs/sessions/2026-10-05-deck-suggestions-analysis.md`.

Codex (2026-10-05, builder focus): added Balanced/Synergy/Missing roles/Early
curve selector with cache invalidation and scoring/filter regressions. Analysis
now shows exact opening-seven land odds before mulligans, with boundary and
normalization tests. All 43 builder tests and app Clippy pass. Handoff remains
`docs/sessions/2026-10-05-deck-suggestions-analysis.md`.

Codex (2026-10-05, tournament builder): implemented imported Modern tournament
observations, similar-list scoring with dates/sample counts/source links, and mana
curve benchmarks. Sidebar cards/suggestions have thumbnails, hover previews, and
click enlargement. Added saved player-declared power, Commander bracket,
competitiveness and Rule 0 profiles with history and JSON interchange. Schema v9
(tournament records) and v10 (deck profiles); existing API preserves profiles.
Full app: 208 passed, 3 ignored; store: 27 passed; app/store Clippy and whitespace
checks pass. Imports are local; no automatic event crawler or corpus shipped.
Handoff: `docs/sessions/2026-10-05-tournament-builder-profiles.md`.

Claude (2026-10-05/06, card coverage): coverage 14,008 → 14,398 / 34,913 (41.2%).
Engine/IR/compiler only, plus one arm in `mtg-policy/src/bot/mod.rs`. New:
- Block requirements (CR 509.1c): `Restriction::{MustBlock, MustBlockSource, MustBeBlocked,
  MustBeBlockedByAll}`; `combat::validate_blocks` now rejects a declaration when a one-step
  change obeys more requirements (`BlockError::RequirementUnmet { better }`), and
  `DeclareBlockers.default` is now a requirement-obeying declaration (`combat::required_blocks`),
  not always empty. The bot uses that default when it's non-empty. **Codex/UI:** a human who
  ignores a lure is silently re-asked; preselecting `choice.default` or showing the error would
  help. Provoke, `Duration::UntilEndOfCombat` (ends at the end-of-combat action and at cleanup).
- `Restriction::AssignAsThoughUnblocked`: answering `AssignCombatDamage` with no rows (or all
  zero) sends all damage to the attacked player/planeswalker. The UI already allows that.
- `AdditionalCost::TapUntapped { filter, count }` ("Tap an untapped creature you control"),
  paid like crew via `ChooseObjects`; not for mana abilities yet.
- `Effect::DealDamageDivided`: one target slot per point of damage, repeats allowed, so the
  split is announced with the targets (CR 601.2d). `Event::Targeted` is now deduped per object.
- Also: `Restriction::CantCast { who, spells, beyond }` (statics and "this turn";
  `cost::cast_forbidden` gates casting, face-down casting and casting mid-resolution);
  `Value::{PartySize, BasicLandTypesAmong, SpellsCastThisTurn}`; undaunted; counter-and-exile
  (`exile` on `CounterSpell`/`CounterUnlessPays`); "shuffles their (hand and) graveyard into
  their library"; "activated abilities of <noun> can't be activated"; "defending player
  controls …" conditions; "whenever you attack with N or more <noun>".
- Tests: `tests/compiled/{block_requirements,tap_costs,divided_damage,cast_restrictions}.rs`.
- 2026-10-06: dredge (`AbilityKind::Dredge`; the draw step can now ask questions via
  `Suspended::Drawing`), blitz, scavenge, encore, surge/spectacle (`AltCost` variants, condition
  in `cost.timing`), awaken (`AltCost::Awaken`; targets on the alternative cost), devour
  (`ReplacementKind::Devour`), casualty/conspire and "Kicker—Sacrifice …" (kicker costs may
  now contain chosen parts), `ObjectFilter::SharesColorWith`. **mtg-app:** four `AltCost`
  labels added in `cards_text.rs` (blitz, surge, spectacle, awaken) to keep it compiling.

Claude (2026-10-06, card coverage): coverage 14,398 → 14,616 / 34,913 (41.9%).
Engine/IR/compiler, plus one `AltCost::Pay` label in `mtg-app/src/cards_text.rs`. New:
- **"Activate only if …"** on any activated ability, mana abilities included: the condition
  goes in `cost.timing`, and `cost::additional_payable` now *evaluates* `cost.timing`
  (`conditions_hold`) instead of refusing any cost that has one. `mana::sources` skips a
  mana ability whose condition fails, so auto-payment won't tap a Tainted land without a
  Swamp. Formidable ("creatures you control have total power N or greater") is a condition.
- **Plain alternative costs** (CR 118.9), `AltCost::Pay`: "You may pay 1 life and exile a blue
  card from your hand / sacrifice two Mountains / tap an untapped creature you control rather
  than pay this spell's mana cost", optionally "If <condition>, …". Its non-mana parts are paid
  like additional costs through the new `cost::spell_extra_cost` (now the single source for
  additional + graveyard-cast + alternative parts). A face has at most one `Pay`.
- `AdditionalCost::ExileFrom { zone: Hand }` is a chosen cost part; announcement exiles those
  (`Announced::exiled`) instead of discarding them.
- Exploit (+ its "when ~ exploits a creature" line → one ETB with a reflexive trigger), For
  Mirrodin!, living metal (static "~ is an artifact creature" for Vehicles; "during turns other
  than yours"), bargain (a kicker whose cost is "sacrifice an artifact, enchantment, or token";
  "if it was bargained" = kicked). "it was kicked/bargained" now refuses when "it" is bound to
  something other than this object.
- Nouns: "token(s)" as a head noun; cost parts no longer split "an artifact, enchantment, or
  token" at its commas; "another creature you control" as a chosen permanent; sacrifice-unless
  accepts "return a land you control to its owner's hand" (Karoos).
- Tests: `crates/mtg-oracle/tests/compiled/written_out.rs` (19). Harness gained a "Rebel"
  subtype.

Claude (2026-10-06, card coverage, continued): 14,616 → 14,750 / 34,913 (42.2%).
- Generic suffixes on any *single* effect (`clauses::guard_suffix`): "… unless you pay {B}{B}"
  (`UnlessPays`), "… unless you pay 2 life", "… unless you <action>" (offered only when
  possible), "… unless <condition>", "… if <condition>" (read on resolution). Compound
  effects (`Sequence`, `Let`, …) are refused, so "discard your hand, then draw that many
  cards if …" stays unparsed.
- Nouns: "<type> token(s)" ("creature tokens you control", "artifact token").
- Mana: "{T}: Add {G} for each creature you control" (`ManaOutput::Repeated` with a counted
  amount; `clauses::scale` handles `AddMana`). `mana::sources` now evaluates such amounts, so
  the planner sees 0 mana from an empty Cradle instead of guessing 1.
- Conditions: "it has a divinity counter on it" / "it has N or more … counters on it".
- **Engine:** random discard now uses `state.rng` (seeded) instead of the first cards in
  hand; the compiler accepts "discard(s) N card(s) at random".
- `compile/tests.rs::mana_lines_with_anything_more_are_left_alone` updated: the "for each Elf"
  line is now compiled. Tests: `tests/compiled/written_out.rs` (29).

Claude (2026-10-06, card coverage, round 3): 14,750 → 14,918 / 34,913 (42.7%).
- `clauses::may`: a subject-less clause gets "you" ("you may gain 3 life"); "you may have X
  deal/get/lose/gain/discard/mill/fight/draw/sacrifice …" is read as "X deals/gets/…"
  (`have_to_does`); "you may pay N life / {E}{E} / {2}. If you do | When you do, …" share one
  branch (`MayPay`).
- "tap or untap <object>": `May { then: Tap, otherwise: Untap }`.
- Trigger subjects: "<noun> deals (combat) damage to an opponent / damage to a player",
  "<noun> becomes the target of a spell or ability an opponent controls", and the same
  damage phrases for "enchanted/equipped creature".
- **Engine:** `state.gained_life_this_turn` + `Condition::YouGainedLifeThisTurn`;
  `state.damaged_by_this_turn` (source, object) + `ObjectFilter::DealtDamageBySelfThisTurn`
  ("a creature dealt damage by this creature this turn dies", noun qualifier). Both cleared
  with the other per-turn sets.
- Ability words added: corrupted, disappear, infusion, opus, renew.
- Tests: `tests/compiled/written_out.rs` (37).

Claude (2026-10-06, card coverage, round 4): 14,918 → 14,972 / 34,913 (42.9%).
- Statics: "As long as enchanted creature / equipped creature / enchanted permanent is …,
  it …" ("it" is the host); condition "enchanted permanent is a creature".
- `Value::CardsDrawnThisTurn` (from `state.draws_this_turn`): "you've drawn two or more
  cards this turn", "for each card you've drawn this turn".
- `Value::{GreatestPower, GreatestToughness, GreatestManaValue}` and `value_phrase` for "the
  greatest power/toughness/mana value among …" and "the total power/toughness of …".
- Self cost: "This spell costs {X} less to cast, where X is <value>".
- Tests: `tests/compiled/written_out.rs` (41). Note: `mtg-app` currently fails to build on
  `play_with_cancel` in `net.rs` — another session's work in progress, not these changes.

Codex (2026-10-06, automatic hosting): implemented nearby mDNS lobby discovery
and embedded ngrok HTTPS tunnels with one-time host token setup, optional assigned
domain, signed tunnel invites, WSS certificate validation, and user instructions
in `docs/hosting.md` linked from the README. Added cancellation of blocked peer
reads so leaving closes the tunnel promptly. The temporary `play_with_cancel`
build failure noted above is resolved. App/net tests: 333 passed, 6 ignored;
real multicast discovery/removal smoke test passed separately. Clippy, formatting,
and GUI build pass. Public ngrok E2E is an explicit ignored test requiring
`NGROK_AUTHTOKEN` (optional `NGROK_DOMAIN`), not run without credentials.

Codex (2026-10-06, tutor resolution): Gamble's whole spell effect was unparsed
because search expected an immediate shuffle after its hand transfer. Search now
preserves intermediate clauses, including random discard; shuffle is a standalone
clause. Also fixed mandatory unrestricted searches, Worldly Tutor's printed "put
the card on top", exact multi-card counts, and "shuffle your library" wording.
Changed `compile/clauses.rs`; six new regressions in `tests/compiled/search.rs`.
Ten search tests and 1,003 engine/oracle/policy tests pass (38 ignored), Clippy
passes. See `docs/sessions/2026-10-06-tutor-resolution.md`. No engine or policy
edits were needed; their existing random-discard implementation works once the
printed effect compiles.

Claude (2026-10-06, card coverage, round 5): 14,972 → 15,065 / 34,913 (43.2%; includes
another session's Gamble work landing meanwhile).
- `ObjectFilter::HasAnyCounter` ("with a counter on it", "counters on them", "no counters
  on it") and `ObjectFilter::DealtDamageThisTurn` ("creature that was dealt damage this
  turn", from `state.damaged_by_this_turn`); "with +1/+1 counters on them".
- `clauses::grants`: "loses all abilities"; "gains K and gets +N/+N"; one-shot effects can
  grant quoted activated/triggered abilities (`quoted_grant`, using `compile::grantable`,
  so "when this dies" grants stay refused); quotes containing "~" are refused.
- Tests: `tests/compiled/written_out.rs` (45).

## 2026-10-06 tutor audit (Codex)

Audited 1,192 distinct library-search faces: 442 compile completely, 750 remain
unsupported. Fixed skipped tutor reveals, public reveal journal projection and
undo boundary, plus duplicate object selection sanitization. Added search edge
case and rendered picker tests; native tutor fixtures and reusable compiler audit
example. See `docs/sessions/2026-10-06-tutor-audit.md` for scope and checks.

Claude (2026-10-06, card coverage, round 6): 15,065 → 15,124 / 34,913 (43.3%).
- `Restriction::AttackDespiteDefender` ("can attack (this turn) as though it/they didn't
  have defender"; checked in `combat` next to the defender test) and "can block an
  additional creature this turn" (`BlockAdditional(Some(1))`, one-shot).
- `Modification::LoseKeyword(k)` (layer 6): removes granted k and printed keyword abilities
  of kind k. `layers::apply_one` now takes the object. Grammar: "loses flying", "lose
  hexproof and indestructible", in one-shot effects and statics.
- Tests: `tests/compiled/written_out.rs` (48).

Claude (2026-10-06, card coverage, round 7): 15,124 → 15,156 / 34,913 (43.4%).
- Nouns: plural three-item "and" lists ("artifacts, creatures, and lands").
- One-shot "<object> can't be regenerated this turn".
- After "return … to the battlefield", "it" is the new permanent (`Binding::It`, which
  `MoveZone` already records).
- A one-shot grant with no duration ("It gains haste.", "gets +2/+2 and gains
  horsemanship") is `Duration::Permanent` (CR 611.2a) — unless the sentence began "Until end
  of turn, …": `Cx::until_eot` carries that through "… and <subject> gets …", reset per
  sentence and per `clauses::effect` call.
- Tests: `tests/compiled/written_out.rs` (50).

## 2026-10-06 release pipeline (Codex)

Added `.github/workflows/release.yml`: each main push computes a conventional
commit semantic version (patch fallback), runs workspace checks, builds Windows
x64 and macOS Intel/Apple Silicon, builds offline in the Flatpak SDK, and publishes
all four assets plus checksums. Whole-workflow queued concurrency prevents version
collisions; retries reuse commit tags. No version-only commits are pushed.

Added `scripts/release.py`, macOS and Flatpak packaging helpers, Flatpak manifest,
AppStream/desktop/icon metadata, and `docs/releases.md`. Windows builds use static
CRT and the GUI subsystem. Apple bundles are ad-hoc signed; developer signing and
notarization need credentials. Local validation: five helper tests, stamped Cargo
metadata with locked dependencies, GUI cargo check, shell syntax, desktop and
AppStream validation, and actionlint. actionlint 1.7.12 lacks GitHub's documented
`concurrency.queue` key; only that known schema error was exempted. Native hosted
platform builds and an actual GitHub release await the first push with these files.

Claude (2026-10-06, card coverage, round 8): 15,156 → 15,175 / 34,913 (43.5%).
- **Tool:** `mtg-cards coverage 0 --blame` prints, for each line that is a card's only
  obstacle, the first sentence at which it stops compiling (`BLAME\t…`). Group them with
  `grep ^BLAME | cut -f2 | sort | uniq -c | sort -rn` to find what to teach next.
- "Choose target <noun> [and target <noun>]." — announces targets, does nothing itself
  (`clauses::choose_target`).
- "… loses N life. You gain life equal to the life lost this way." (amount × players).
- Reveal-and-choose: "a noncreature, nonland card", "a card from it with mana value N or
  greater", ", you choose …, then that player discards that card".
- Tests: `tests/compiled/written_out.rs` (53).

Claude (2026-10-06, card coverage, round 9): 15,175 → 15,216 / 34,913 (43.6%).
- `dig`: "up to two / any number of <noun> cards … and put them / the revealed cards into
  your hand"; colour/adjective-only nouns ("a white card", "a historic card", "a
  noncreature, nonland card", "a permanent card"); "with power 2 or less" after "card";
  "put one into your hand and the other …"; a bare "on the bottom of your library".
- Mana: "Add two/three mana of any one color" (`Repeated` of `AnyOf`). "In any combination
  of colors" deliberately not read: an activation records one colour choice.
- **Engine fix:** `Effect::AddMana` resolution now evaluates counted amounts
  (`mana::counted`, now `pub(crate)`). Before, a manually activated or triggered "add {G}
  for each creature you control" made 1 mana; auto-payment was already right.
- Tests: `tests/compiled/written_out.rs` (56).

## 2026-10-06 ngrok browser links (Codex)

Enabled eframe `links`: default features were disabled, so the existing hosting
account/authtoken/domain hyperlinks emitted URL commands without opening a
browser. Browser dependency is now locked; documented returning from the
browser to paste the token. Verified the feature chain and GUI cargo check.

Claude (2026-10-06, card coverage, round 10): 15,216 → 15,244 / 34,913 (43.7%).
- Prevention: "… combat damage that would be dealt to and dealt by <object> this turn"
  (two shields).
- `Cx::group`: after a group effect ("creatures you control get +1/+1 until end of
  turn"), "them" / "those creatures" name that group ("Untap them.").
- `Restriction::AssignDamageByToughness` and `combat::damage_amount` — every place combat
  damage reads power (combat.rs assignments, engine.rs damage choices) now goes through it.
- Tests: `tests/compiled/written_out.rs` (59).

## 2026-10-06 Windows application paths (Codex)

Added shared `mtg-store::paths` resolution. Windows data/database and image
cache use LOCALAPPDATA; artwork configuration uses APPDATA. Missing native
variables fall back through USERPROFILE/HOME to the Windows temporary directory,
never the working directory. Existing Unix locations and MTGO_RS_DB/XDG overrides
remain supported, empty variables are ignored, and OS strings preserve paths.
GUI artwork paths delegate to the shared module; CLI database defaults do too.
Documented older Windows database migration without moving user files. Added
native path tests to desktop release jobs. Local checks passed: 33 store tests,
14 artwork tests, Clippy for store/app, GUI build, workflow lint and diff check.
Native Windows execution remains a release-runner check.

Claude (2026-10-06, card coverage, round 11): 15,244 → 15,266 / 34,913 (43.7%).
- **Engine:** `ZonePosition::OwnerChooses` is now carried out (the owner is asked top or
  bottom). Grammar: "<object>'s owner puts it on their choice of the top or bottom of their
  library".
- "Shuffle ~ into its owner's library." (move this card, then its owner shuffles).
- "that token" names the token just made (like "it").
- Tests: `tests/compiled/written_out.rs` (62).
