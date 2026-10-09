//! Ragavan: "Whenever Ragavan deals combat damage to a player, create a Treasure token and
//! exile the top card of that player's library. Until end of turn, you may cast that
//! card." — another player's card, cast by Ragavan's controller.

use super::harness::*;
use mtg_core::{Step, Zone, ZoneRef};
use mtg_engine::actions::Action;

const RAGAVAN: &str = "Whenever this creature deals combat damage to a player, create a \
                       Treasure token and exile the top card of that player's library. Until \
                       end of turn, you may cast that card.";

#[test]
fn it_steals_the_top_card_to_cast() {
    let mut table = Table::default();
    let ragavan = table.card(
        "{R}",
        "Legendary Creature — Monkey Pirate",
        Some((2, 1)),
        RAGAVAN,
    );
    let sorcery = table.card("{0}", "Sorcery", None, "You gain 3 life.");
    let mut game = Game::new(table);
    let ragavan = game.put(ragavan, P0, Zone::Battlefield);
    let top = game.put(sorcery, P1, Zone::Library);
    let order = game
        .engine
        .state
        .zone_order
        .get_mut(&ZoneRef::of(Zone::Library, P1))
        .unwrap();
    order.retain(|o| *o != top);
    order.insert(0, top);
    game.main();
    game.combat(&[ragavan], &[], &[], &[]);
    assert_eq!(game.life(P1), 18);
    let exiled: Vec<_> = game
        .engine
        .state
        .objects_in(ZoneRef::shared(Zone::Exile))
        .into_iter()
        .filter(|id| game.engine.state.objects[id].owner == P1)
        .collect();
    assert_eq!(exiled.len(), 1, "P1's top card is exiled");
    let treasures = game
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| game.engine.state.objects[id].is_token)
        .count();
    assert_eq!(treasures, 1);
    let actions = game.until(P0, Step::PostcombatMain);
    assert!(
        actions.contains(&Action::Cast { object: exiled[0] }),
        "P0 may cast P1's card"
    );
    game.cast(exiled[0], &[]);
    assert_eq!(game.life(P0), 23);
    assert_eq!(
        game.count(Zone::Graveyard, P1),
        1,
        "it goes to its owner's graveyard"
    );
}
