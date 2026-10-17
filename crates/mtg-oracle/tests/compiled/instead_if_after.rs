//! "Metalcraft — ~ deals 4 damage instead if you control three or more artifacts."
//! (Galvanic Blast): the condition written after the replacement reads like "if …, …
//! instead".

use super::harness::*;
use mtg_core::{Target, Zone};

const BLAST: &str = "~ deals 2 damage to any target.
Metalcraft — ~ deals 4 damage instead if you control three or more artifacts.";

#[test]
fn metalcraft_decides_the_damage() {
    for (artifacts, dealt) in [(2, 2), (3, 4)] {
        let mut t = Table::default();
        let blast = t.card("{R}", "Instant", None, BLAST);
        let rock = t.card("{1}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(1);
        let blast = g.put(blast, P0, Zone::Hand);
        for _ in 0..artifacts {
            g.put(rock, P0, Zone::Battlefield);
        }
        g.main();
        let before = g.life(P1);
        g.cast(blast, &[Target::Player(P1)]);
        assert_eq!(before - g.life(P1), dealt, "{artifacts} artifacts");
    }
}

#[test]
fn in_one_line_too() {
    for (died, dealt) in [(false, 3), (true, 5)] {
        let mut t = Table::default();
        let flames = t.card(
            "{R}",
            "Instant",
            None,
            "~ deals 3 damage to any target. ~ deals 5 damage instead if a creature died \
             this turn.",
        );
        let kill = t.card("{0}", "Instant", None, "Destroy target creature.");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let flames = g.put(flames, P0, Zone::Hand);
        let kill = g.put(kill, P0, Zone::Hand);
        let bear = g.put(bear, P1, Zone::Battlefield);
        g.main();
        if died {
            g.cast(kill, &[Target::Object(bear)]);
        }
        let before = g.life(P1);
        g.cast(flames, &[Target::Player(P1)]);
        assert_eq!(before - g.life(P1), dealt, "died: {died}");
    }
}
