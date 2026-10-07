//! "If ~ would be put into a graveyard from anywhere, reveal ~ and shuffle it into its
//! owner's library instead."

use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};
use mtg_engine::actions::Action;

const TEXT: &str = "If ~ would be put into a graveyard from anywhere, reveal ~ and shuffle it \
                    into its owner's library instead.";

fn shuffled(game: &Game, player: mtg_core::PlayerId) -> bool {
    game.engine
        .log
        .iter()
        .any(|e| e.event == mtg_core::Event::Shuffled { player })
}

#[test]
fn destroyed_it_is_shuffled_into_the_library() {
    let mut table = Table::default();
    let titan = table.card("{4}{B}", "Creature — Bear", Some((3, 3)), TEXT);
    let murder = table.card("{B}", "Instant", None, "Destroy target creature.");
    let mut game = Game::new(table);
    game.lands(1);
    let titan = game.put(titan, P1, Zone::Battlefield);
    let murder = game.put(murder, P0, Zone::Hand);
    game.main();
    let library = game.count(Zone::Library, P1);
    game.act(
        Action::Cast { object: murder },
        &[Target::Object(titan)],
        &[],
    );
    assert_eq!(game.count(Zone::Graveyard, P1), 0);
    assert_eq!(
        game.count(Zone::Library, P1),
        library + 1,
        "into its owner's library"
    );
    assert!(shuffled(&game, P1));
    assert_eq!(
        game.count(Zone::Graveyard, P0),
        1,
        "the spell itself still goes there"
    );
}

#[test]
fn discarded_it_is_shuffled_into_the_library() {
    let mut table = Table::default();
    let titan = table.card("{4}{B}", "Creature — Bear", Some((3, 3)), TEXT);
    let mind_rot = table.card("{B}", "Sorcery", None, "Target player discards a card.");
    let mut game = Game::new(table);
    game.lands(1);
    game.put(titan, P1, Zone::Hand);
    let spell = game.put(mind_rot, P0, Zone::Hand);
    game.main();
    let library = game.count(Zone::Library, P1);
    game.act(Action::Cast { object: spell }, &[Target::Player(P1)], &[]);
    assert_eq!(game.count(Zone::Graveyard, P1), 0);
    assert_eq!(
        game.engine
            .state
            .objects_in(ZoneRef::of(Zone::Library, P1))
            .len(),
        library + 1
    );
    assert!(shuffled(&game, P1));
}
