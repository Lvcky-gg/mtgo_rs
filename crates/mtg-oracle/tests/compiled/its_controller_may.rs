//! "Its controller may search their library for a basic land card, …": the other player
//! decides and searches their own library (`Effect::AsPlayer`).

use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

const PATH: &str = "Exile target creature. Its controller may search their library for a basic \
                    land card, put that card onto the battlefield tapped, then shuffle.";

fn path_game() -> (
    Game,
    mtg_core::ObjectId,
    mtg_core::ObjectId,
    mtg_core::ObjectId,
) {
    let mut table = Table::default();
    let path = table.card("{W}", "Instant", None, PATH);
    let mountain = table.mountain();
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(1);
    let path = game.put(path, P0, Zone::Hand);
    let bear = game.put(bear, P1, Zone::Battlefield);
    let mountain = game.put(mountain, P1, Zone::Library);
    game.main();
    (game, path, bear, mountain)
}

fn p1_lands(game: &Game) -> Vec<mtg_core::ObjectId> {
    game.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            game.engine.state.objects[id].controller == P1
                && mtg_engine::layers::compute(&game.engine.state, &game.table, *id)
                    .is_some_and(|c| c.has_type(mtg_core::CardType::Land))
        })
        .collect()
}

#[test]
fn the_exiled_creatures_controller_finds_a_basic() {
    let (mut game, path, bear, mountain) = path_game();
    let library = game.count(Zone::Library, P1);
    game.act(
        Action::Cast { object: path },
        &[Target::Object(bear)],
        &[Answer::Bool(true), Answer::Objects(vec![mountain])],
    );
    let lands = p1_lands(&game);
    assert_eq!(lands.len(), 1, "P1 put a basic onto the battlefield");
    assert!(game.engine.state.objects[&lands[0]].tapped);
    assert_eq!(game.count(Zone::Library, P1), library - 1);
    assert!(
        game.engine
            .log
            .iter()
            .any(|e| e.event == mtg_core::Event::Shuffled { player: P1 })
    );
    assert_eq!(
        game.engine
            .state
            .objects_in(ZoneRef::shared(Zone::Exile))
            .len(),
        1
    );
}

#[test]
fn declining_finds_nothing() {
    let (mut game, path, bear, _) = path_game();
    game.act(
        Action::Cast { object: path },
        &[Target::Object(bear)],
        &[Answer::Bool(false)],
    );
    assert!(p1_lands(&game).is_empty());
}

#[test]
fn each_player_puts_a_card_from_their_own_hand() {
    let mut table = Table::default();
    let show = table.card(
        "{2}{U}",
        "Sorcery",
        None,
        "Each player may put an artifact, creature, enchantment, or land card from their hand \
         onto the battlefield.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(3);
    let show = game.put(show, P0, Zone::Hand);
    let mine = game.put(bear, P0, Zone::Hand);
    let theirs = game.put(bear, P1, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: show },
        &[],
        &[
            Answer::Bool(true),
            Answer::Objects(vec![mine]),
            Answer::Bool(true),
            Answer::Objects(vec![theirs]),
        ],
    );
    let on_battlefield = |owner| {
        game.engine
            .state
            .battlefield()
            .into_iter()
            .filter(|id| {
                let o = &game.engine.state.objects[id];
                o.owner == owner && o.controller == owner
            })
            .filter(|id| {
                mtg_engine::layers::compute(&game.engine.state, &game.table, *id)
                    .is_some_and(|c| c.has_type(mtg_core::CardType::Creature))
            })
            .count()
    };
    assert_eq!(on_battlefield(P0), 1, "my bear");
    assert_eq!(on_battlefield(P1), 1, "their bear, under their control");
}

#[test]
fn each_player_searches_their_own_library_when_it_dies() {
    let mut table = Table::default();
    let explorer = table.card(
        "{G}",
        "Creature — Bear",
        Some((1, 1)),
        "When this creature dies, each player may search their library for up to two basic \
         land cards, put them onto the battlefield, then shuffle.",
    );
    let murder = table.card("{B}", "Instant", None, "Destroy target creature.");
    let mountain = table.mountain();
    let mut game = Game::new(table);
    game.lands(1);
    let explorer = game.put(explorer, P0, Zone::Battlefield);
    let murder = game.put(murder, P0, Zone::Hand);
    let mine: Vec<_> = (0..2)
        .map(|_| game.put(mountain, P0, Zone::Library))
        .collect();
    let theirs: Vec<_> = (0..2)
        .map(|_| game.put(mountain, P1, Zone::Library))
        .collect();
    game.main();
    game.act(
        Action::Cast { object: murder },
        &[Target::Object(explorer)],
        &[
            Answer::Bool(true),
            Answer::Objects(mine),
            Answer::Bool(true),
            Answer::Objects(theirs),
        ],
    );
    assert_eq!(p1_lands(&game).len(), 2, "P1 found two in their library");
    for p in [P0, P1] {
        assert!(
            game.engine
                .log
                .iter()
                .any(|e| e.event == mtg_core::Event::Shuffled { player: p })
        );
    }
}

#[test]
fn target_opponent_exiles_a_creature_of_their_own() {
    let mut table = Table::default();
    let edict = table.card(
        "{1}{W}",
        "Sorcery",
        None,
        "Target opponent exiles a creature they control.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(2);
    let edict = game.put(edict, P0, Zone::Hand);
    let mine = game.put(bear, P0, Zone::Battlefield);
    let theirs = game.put(bear, P1, Zone::Battlefield);
    game.main();
    game.act(
        Action::Cast { object: edict },
        &[Target::Player(P1)],
        &[Answer::Objects(vec![theirs])],
    );
    assert!(
        !game.engine.state.objects.contains_key(&theirs),
        "P1's exiled"
    );
    assert!(game.engine.state.objects.contains_key(&mine), "not mine");
    assert_eq!(
        game.engine
            .state
            .objects_in(ZoneRef::shared(Zone::Exile))
            .len(),
        1
    );
}
