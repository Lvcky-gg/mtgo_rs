# Deck coverage and partner commanders — 2026-10-15, round 2

Deck compilation coverage rises from 162 to 171 / 209 unique priorities. Newly
supported: Narcomoeba; Samut, Tyrant of Naktamun; Vial Smasher the Fierce;
Windfall; Mana Drain; Narset, Parter of Veils; Forbidden Orchard;
Cayth, Famed Mechanist; and The One Ring. The refreshed ordered blocker report
is `docs/card-coverage/deck-priorities-report.txt`: 32 blocked, six absent from
the local catalog. Thrasios, Vial Smasher, and Cayth now compile; Araumi is still
blocked. Existing uncommitted work is preserved; changes remain uncommitted.

Whole-catalog compilation coverage on the same SQLite snapshot rises from
17,925 to 17,965 / 34,954 (51.4%). This is a net count, not a complete global
card-name regression comparison. Compilation coverage is not a full rules audit.

## Partner commanders

The app and CLI accept up to two individually designated commanders. Companion
designations, three commanders, and multiple copies of one commander remain
rejected before replacing a saved deck. The existing database section holds
both cards without a schema migration. Older portable decks remain readable;
the new optional partner field is omitted when empty.

The builder can select a partner from card context menus, display and remove
that slot, save/load it, undo/redo it, and export/import it through every
supported text/structured format. Card coverage, suggestions, color identity,
printing selection, and match card-table construction include both commanders.
Validation requires two distinct legendary creatures with the printed Partner
ability, 98 main-deck cards, and the union of their color identities. This
implements ordinary Partner, including Thrasios/Vial Smasher; other pairing
mechanics such as Partner with and Choose a Background are not implemented.

Game setup puts both commanders in the command zone. Both can be cast and
returned there from graveyard/exile. Tax and combat damage are tracked separately
for each commander; two commanders dealing 12 each is not lethal commander
damage. Command Tower/Arcane Signet use their combined identities even after a
commander leaves the command zone. Public views and board labels distinguish
primary and partner damage. Existing single-commander state representations
retain their fields and omit empty partner maps in verification snapshots.

Rules reference: [Commander 2016 mechanics](https://magic.wizards.com/en/news/feature/commander-2016-mechanics-2016-10-24).

## Card behavior

Narcomoeba uses a self zone-change trigger functioning in the graveyard and
returns only when milled, with the choice made on resolution. Samut grants split
second to your instant/sorcery spells on the stack, preserving mana abilities.

Vial Smasher selects one opponent with the seeded game RNG, then offers that
player or a planeswalker they control as a non-targeting resolution choice.
Neither hexproof nor shroud prevents that choice. The first-spell trigger works
on opponents' turns too. Spell mana values now include announced X on the stack
and when reading that spell's last-known information after countering it.
Short self references before a "the" title are normalized like comma titles.

Discard instructions bind the cards actually discarded. Windfall draws the
largest count owned by any one player, after everyone discards, including zero
and ties. Narset checks each draw against the whole turn's count, including
cards drawn before she entered; the cap resets each turn and ends when she
leaves. Forbidden Orchard observes activation of a mana ability with a tap cost,
not ordinary tapping or activation of a nonmana tap ability.

Mana Drain freezes its numerical amount before creating its delayed trigger,
so it survives cleanup clearing last-known information. The next main phase can
be your postcombat main phase; opponents' main phases do not trigger it. Mana is
added even if the target could not be countered.

Cayth grants the existing fabricate trigger to other nontoken creatures you
control. Activated modal abilities now parse costs and modes; pre-activation
legality checks consider only the necessary modes' targets. Modal mana abilities
remain conservatively unsupported. The One Ring gains its protection only when
cast, draws using the burden count after adding a counter, loses life at upkeep,
and loses protection at the beginning of its controller's next turn.

## Verification

New regressions cover partner validation/setup, older deck deserialization,
independent tax/damage, graveyard/exile return, combined Command Tower identity,
builder history and all export formats, plus compiled gameplay tests for each
newly supported card. Vial Smasher tests include repeatable random choices with
planeswalker recipient prompts, X spells, and countered spells. Cayth tests
include both modes and an ability whose unchosen mode has no legal target.

Final review also caught and fixed a source-identity bug: after a permanent left,
its pending ability treated the ability's stack object as its source and could
put counters on it. Abilities now retain their actual source identity, numerical
and keyword reads use last-known information, and counter placement ignores
missing or phased-out objects. Zone changes preserve source snapshots when a
pending ability needs them, including cards exiled from a hand or graveyard as
activation costs. Existing encore, eternalize, and scavenge regressions pass.
New Ring tests cover bouncing it in response to its draw ability (draw using the
old burden count, without a phantom counter event) and bouncing it before its
cast-conditioned enter trigger resolves (protection still granted).

Validation passes: `cargo test --workspace --quiet` outside the sandbox for the
existing loopback tests; the final expanded Ring, Mana Drain, Samut, Vial Smasher,
and Cayth gameplay regressions; `cargo clippy --workspace --all-targets -- -D
warnings`; `cargo fmt --all -- --check`; and `git diff --check`. No existing
replay or regression fixture was updated.
