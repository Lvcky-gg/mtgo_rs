//! Fellwar Stone, Exotic Orchard: "{T}: Add one mana of any color that a land an opponent
//! controls could produce." (CR 106.7)

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

const FELLWAR: &str =
    "{T}: Add one mana of any color that a land an opponent controls could produce.";

/// Which of {W}, {U}, {B}, {R}, {G} spells P0 can cast with a Fellwar Stone, given the
/// mana texts of P1's lands.
fn castable(p1_lands: &[&str]) -> [bool; 5] {
    let mut table = Table::default();
    let stone = table.card("{2}", "Artifact", None, FELLWAR);
    let lands: Vec<_> = p1_lands
        .iter()
        .map(|t| table.card("", "Land", None, t))
        .collect();
    let spells: Vec<_> = ["{W}", "{U}", "{B}", "{R}", "{G}"]
        .into_iter()
        .map(|c| table.card(c, "Instant", None, "You gain 1 life."))
        .collect();
    let mut game = Game::new(table);
    game.put(stone, P0, Zone::Battlefield);
    for l in lands {
        game.put(l, P1, Zone::Battlefield);
    }
    let spells: Vec<_> = spells
        .into_iter()
        .map(|s| game.put(s, P0, Zone::Hand))
        .collect();
    let actions = game.main();
    let mut out = [false; 5];
    for (i, s) in spells.iter().enumerate() {
        out[i] = actions.contains(&Action::Cast { object: *s });
    }
    out
}

#[test]
fn the_colors_their_lands_could_make() {
    assert_eq!(
        castable(&["{T}: Add {G}.", "{T}: Add {W} or {U}."]),
        [true, true, false, false, true]
    );
}

#[test]
fn no_lands_no_mana() {
    assert_eq!(castable(&[]), [false; 5]);
}

#[test]
fn two_orchards_do_not_feed_each_other() {
    assert_eq!(castable(&[FELLWAR]), [false; 5]);
}
