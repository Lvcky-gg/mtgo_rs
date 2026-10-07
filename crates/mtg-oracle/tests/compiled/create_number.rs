//! "Create a number of … tokens equal to …".

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

fn tokens(game: &Game) -> Vec<mtg_core::ObjectId> {
    game.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| game.engine.state.objects[id].is_token)
        .collect()
}

#[test]
fn the_count_is_read_as_it_resolves() {
    let mut table = Table::default();
    let krenko = table.card(
        "{2}{R}",
        "Creature — Goblin",
        Some((1, 2)),
        "Whenever this creature attacks, put a +1/+1 counter on it, then create a number of \
         1/1 red Goblin creature tokens equal to this creature's power.",
    );
    let mut game = Game::new(table);
    let krenko = game.put(krenko, P0, Zone::Battlefield);
    game.main();
    game.combat(&[krenko], &[], &[], &[]);
    assert_eq!(tokens(&game).len(), 2, "power 2 after the counter");
}

#[test]
fn tapped_tokens_for_a_dead_creatures_power() {
    let mut table = Table::default();
    let hydra = table.card(
        "{3}{G}",
        "Creature — Hydra",
        Some((3, 3)),
        "When this creature dies, create a number of tapped Treasure tokens equal to its power.",
    );
    let murder = table.card("{B}", "Instant", None, "Destroy target creature.");
    let mut game = Game::new(table);
    game.lands(1);
    let hydra = game.put(hydra, P0, Zone::Battlefield);
    let murder = game.put(murder, P0, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: murder },
        &[Target::Object(hydra)],
        &[],
    );
    let made = tokens(&game);
    assert_eq!(made.len(), 3);
    assert!(made.iter().all(|id| game.engine.state.objects[id].tapped));
}
