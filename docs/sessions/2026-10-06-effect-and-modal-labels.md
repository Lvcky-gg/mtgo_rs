# Effect and modal labels

Effect entry now uses the existing explicit strip_ability_word list, fixing labels
on spell effect lines and individual modal choices. Added Cure Wounds, Dispel
Magic, Gentle Repose, Sort Inventory, Pick a Perk, Check Map to that list.
Unknown prefixes remain rejected; conditions following labels are fully parsed.
No engine/IR changes.

Frozen comparison: 16,084 → 16,125 / 34,913 (46.2%), 41 gains, zero losses.
Includes Dawnbringer Cleric, Pip-Boy 3000, Dispatch, Painful Truths, Radiant Flames,
Herd Migration, Shamanic Revelation, Wild Slash, Descend upon the Sinful.
Snapshot /tmp/mtgo-modal-labels-comparison; full gain list obtainable from
/tmp/mtgo-modal-labels-before.txt and /tmp/mtgo-modal-labels-after.txt.

Two parameterized gameplay tests in labeled_modes.rs verify all three Dawnbringer
modes and their distinct targets, and Dispatch with two versus three artifacts:
only tap without metalcraft, tap and exile with metalcraft.
Oracle suite and all-target Clippy pass; 15 existing ignored tests.
