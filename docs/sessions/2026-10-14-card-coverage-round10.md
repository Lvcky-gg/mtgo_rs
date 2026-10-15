# Card coverage, round 10 — 2026-10-14

Using the same `/tmp/card-coverage.sqlite` snapshot, coverage rises from 17,713
to 17,744 / 34,914 (50.8%). The complete card-name comparison gains 31 cards
and loses none. The existing five-card name-lookup difference remains
(17,749 versus 17,744).

## Comparison filters

Power, toughness, and mana-value qualities accept `less than`, `greater than`,
and their `or equal to` variants. Bounds can use existing explicit values such
as the source's power, the controller's life total, and counts of cards or lands.
Strict comparisons negate the corresponding inclusive bound. References to
`its` or `their` characteristics remain unsupported here because they would
require a value relative to each filtered object.

Trigger detection evaluates comparison values in the trigger source's context
rather than replacing every nonconstant value with zero. Restrictions consulted
outside a layer pass use current computed characteristics. Blocking filters use
the effect's source/controller, preserving the meaning of `you` for restrictions
imposed on another player's creatures.

Self-death triggers keep their existing new-zone source identity for self-return
effects. Source-relative power/toughness in their target bounds are captured from
the old object's last-known characteristics in a per-trigger ability snapshot.
Only target bounds are rewritten; nested granted abilities retain their own source.

Newly playable: Alesha, Who Laughs at Fate; Ayesha Tanaka, Armorer;
Beguiler of Wills; Beseech the Queen; Blazing Hope; Champion of Lambholt;
Dominating Vampire; Earthshaker Khenra; Ensnaring Bridge; Fiendish Panda;
Goma Fada Vanguard; Grim Servant; Gurgling Anointer; Hammerhead Tyrant;
Lay Down Arms; Legacy's Allure; Lena, Selfless Champion; Loot, Exuberant Explorer;
Mandate of Abaddon; Mausoleum Secrets; Nightmare Unmaking; Pollywog Prodigy;
Spawnbroker; Spitfire Handler; Stone Giant; Sundering Archaic; Temporary Insanity;
Unliving Psychopath; Wasp, Shrinking Savior; Yasova Dragonclaw;
Zar Ojanen, Scion of Efrava.

Seven compiled gameplay regressions cover all four comparison boundaries for
power and toughness, Ensnaring Bridge's live hand count, Champion of Lambholt's
current power and controller-relative blocking restriction, Beseech the Queen's
land-count search bound, Earthshaker Khenra's target recheck, Fiendish Panda's
last-known pumped power, and Pollywog Prodigy's dynamic cast filter including
counters on its source.

Validation: targeted compiled tests; `cargo test --workspace --quiet` including
the replay/regression corpus; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all --check`; and `git diff --check`. Workspace tests run outside
the sandbox for their existing loopback networking tests. Changes remain uncommitted.
