//! CR 700.2d — "Choose three. You may choose the same mode more than once." (Fiery
//! Confluence): a mode can be picked again, and each pick has its own targets.

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;
use mtg_engine::choice::Answer;

const CONFLUENCE: &str = "Choose three. You may choose the same mode more than once.
• ~ deals 1 damage to each creature.
• ~ deals 2 damage to each opponent.
• Destroy target artifact.";

// The picks, in order: [1,1,1] [1,1,2] [1,1,3] [1,2,2] [1,2,3] [1,3,3] [2,2,2] [2,2,3]
// [2,3,3] [3,3,3].
const THREE_BURNS: u8 = 6;
const BURN_AND_TWO_ARTIFACTS: u8 = 8;
const THREE_PINGS: u8 = 0;

#[test]
fn the_same_mode_three_times() {
    let mut t = Table::default();
    let spell = t.card("{0}", "Sorcery", None, CONFLUENCE);
    let mut g = Game::new(t);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let before = g.life(P1);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Modes(vec![THREE_BURNS])],
    );
    assert_eq!(g.life(P1), before - 6);
}

#[test]
fn each_copy_of_a_mode_has_its_own_target() {
    let mut t = Table::default();
    let spell = t.card("{0}", "Sorcery", None, CONFLUENCE);
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    let spell = g.put(spell, P0, Zone::Hand);
    let a = g.put(rock, P1, Zone::Battlefield);
    let b = g.put(rock, P1, Zone::Battlefield);
    g.main();
    let before = g.life(P1);
    g.act(
        Action::Cast { object: spell },
        &[Target::Object(a), Target::Object(b)],
        &[Answer::Modes(vec![BURN_AND_TWO_ARTIFACTS])],
    );
    assert_eq!(g.life(P1), before - 2);
    assert_eq!(g.count(Zone::Graveyard, P1), 2);
}

#[test]
fn three_pings_kill_a_three_toughness_creature() {
    let mut t = Table::default();
    let spell = t.card("{0}", "Sorcery", None, CONFLUENCE);
    let ogre = t.card("{2}{R}", "Creature — Ogre", Some((3, 3)), "");
    let mut g = Game::new(t);
    let spell = g.put(spell, P0, Zone::Hand);
    g.put(ogre, P1, Zone::Battlefield);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Modes(vec![THREE_PINGS])],
    );
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
}
