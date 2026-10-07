//! "Enchanted creature has protection from green. This effect doesn't remove this Aura."

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

fn enchant(
    text: &str,
) -> (
    Game,
    mtg_core::ObjectId,
    mtg_core::ObjectId,
    mtg_core::ObjectId,
) {
    let mut table = Table::default();
    let aura = table.card("{G}", "Enchantment — Aura", None, text);
    let growth = table.card(
        "{G}",
        "Instant",
        None,
        "Target creature gets +3/+3 until end of turn.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(2);
    let aura = game.put(aura, P0, Zone::Hand);
    let growth = game.put(growth, P0, Zone::Hand);
    let bear = game.put(bear, P0, Zone::Battlefield);
    game.main();
    game.act(Action::Cast { object: aura }, &[Target::Object(bear)], &[]);
    let attached = game
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| game.engine.state.objects[id].attached_to == Some(bear));
    (game, bear, growth, attached.unwrap_or(aura))
}

#[test]
fn the_aura_stays_and_the_protection_holds() {
    let (mut game, bear, growth, aura) = enchant(
        "Enchant creature\nEnchanted creature has protection from green. This effect doesn't \
         remove this Aura.",
    );
    assert!(
        game.engine.state.objects.contains_key(&aura),
        "the green Aura stays on"
    );
    assert_eq!(game.count(Zone::Graveyard, P0), 0);
    let actions = game.main();
    assert!(
        !actions.contains(&Action::Cast { object: growth }),
        "no green spell can target the bear: {bear:?}"
    );
}

#[test]
fn without_the_clause_the_aura_falls_off() {
    let (game, _, _, _) =
        enchant("Enchant creature\nEnchanted creature has protection from green.");
    assert_eq!(
        game.count(Zone::Graveyard, P0),
        1,
        "the green Aura fell off"
    );
}
