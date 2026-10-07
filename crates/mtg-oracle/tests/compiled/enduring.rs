//! Enduring (Duskmourn): "When ~ dies, if it was a creature, return it to the battlefield
//! under its owner's control. It's an enchantment."

use super::harness::*;
use mtg_core::{CardType, Target, Zone};
use mtg_engine::actions::Action;

#[test]
fn it_comes_back_as_an_enchantment_only() {
    let mut table = Table::default();
    let glimmer = table.card(
        "{2}{W}",
        "Enchantment Creature — Bear",
        Some((2, 2)),
        "When ~ dies, if it was a creature, return it to the battlefield under its owner's \
         control. It's an enchantment.",
    );
    let murder = table.card("{B}", "Instant", None, "Destroy target creature.");
    let mut game = Game::new(table);
    game.lands(1);
    let glimmer_id = game.put(glimmer, P1, Zone::Battlefield);
    let murder = game.put(murder, P0, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: murder },
        &[Target::Object(glimmer_id)],
        &[],
    );
    let back = game.find(glimmer).expect("returned to the battlefield");
    assert_eq!(
        game.engine.state.objects[&back].zone.zone,
        Zone::Battlefield
    );
    let ch = game
        .engine
        .characteristics(&game.table, back)
        .unwrap()
        .clone();
    assert!(ch.has_type(CardType::Enchantment));
    assert!(!ch.has_type(CardType::Creature), "no longer a creature");
}
