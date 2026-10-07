# Source untap triggers — 2026-10-06

Added “becomes untapped” to the compiler's source-event phrase table, mapping to
the existing BecomesUntapped event pattern. No engine or IR changes were needed.
This completes Pain Seer's missing trigger following the reveal-to-hand batch.

Frozen-source comparison: 15,846 → 15,864 / 34,913 (45.4%), +18, zero losses.
Added Aerie Worshippers, Deepwater Hypnotist, Felhide Spiritbinder, Forlorn
Pseudamma, God-Favored General, Hollowsage, Key to the City, Kragma Butcher,
Oreskos Sun Guide, Pain Seer, Pheres-Band Raiders, Pheres-Band Tromper,
Satyr Nyx-Smith, Servant of Tymaret, Siren of the Silent Song, Sphinx's Disciple,
Warchanter of Mogis, Well Rested. Claude's concurrent changes were held constant.

Three gameplay regressions cover spell untaps, an already untapped source,
stun-counter replacement, triggers queued during the normal untap step before
the draw, and a source-only temporary pump. The Pain Seer tests verify reveal,
card movement, and the correct life loss together with its newly enabled trigger.

Validation: Oracle suite 839 passed, 15 existing ignored; Clippy with
warnings denied and diff check pass. Generic permanent untap triggers such as
Mesmeric Orb remain outside this source-phrase change.

Changes confined to the source-event table in compile/mod.rs,
compiled/untap_triggers.rs plus registration, README and coordination docs.
