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

#[test]
fn create_x_tokens_where_x_is_counted_and_from_an_x_cost() {
    let mut table = Table::default();
    let boss = table.card(
        "{2}{R}{R}",
        "Creature — Goblin",
        Some((3, 3)),
        "{T}: Create X 1/1 red Goblin creature tokens, where X is the number of Goblins you \
         control.",
    );
    let offensive = table.card(
        "{X}{1}{R}{R}",
        "Sorcery",
        None,
        "Create X 1/1 red Goblin creature tokens.",
    );
    let goblin = table.card("{R}", "Creature — Goblin", Some((1, 1)), "");
    let mut game = Game::new(table);
    game.lands(5);
    let boss = game.put(boss, P0, Zone::Battlefield);
    game.put(goblin, P0, Zone::Battlefield);
    let offensive = game.put(offensive, P0, Zone::Hand);
    game.main();
    game.act(activate(boss, 0), &[], &[]);
    assert_eq!(
        tokens(&game).len(),
        2,
        "two Goblins: the boss and one other"
    );
    game.act(
        Action::Cast { object: offensive },
        &[],
        &[mtg_engine::choice::Answer::Number(2)],
    );
    assert_eq!(tokens(&game).len(), 4, "X = 2 more");
}

#[test]
fn martial_coup_keeps_the_tokens_it_made() {
    let mut table = Table::default();
    let coup = table.card(
        "{X}{W}{W}",
        "Sorcery",
        None,
        "Create X 1/1 white Soldier creature tokens. If X is 5 or more, destroy all other \
         creatures.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(7);
    let coup = game.put(coup, P0, Zone::Hand);
    let theirs = game.put(bear, P1, Zone::Battlefield);
    game.main();
    game.act(
        Action::Cast { object: coup },
        &[],
        &[mtg_engine::choice::Answer::Number(5)],
    );
    assert!(
        !game.engine.state.objects.contains_key(&theirs),
        "the bear is destroyed"
    );
    assert_eq!(
        tokens(&game).len(),
        5,
        "the soldiers are not 'other' creatures"
    );
}

#[test]
fn that_many_tokens_and_doubling_counters() {
    let mut table = Table::default();
    let nest = table.card(
        "{2}{G}",
        "Creature — Bear",
        Some((0, 4)),
        "Whenever this creature is dealt damage, create that many 1/1 green Bear creature \
         tokens.",
    );
    let brute = table.card("{2}{R}", "Creature — Bear", Some((3, 3)), "");
    let curve = table.card(
        "{G}",
        "Sorcery",
        None,
        "Put a +1/+1 counter on target creature you control, then double the number of \
         +1/+1 counters on that creature.",
    );
    let mut game = Game::new(table);
    game.lands(1);
    let brute = game.put(brute, P0, Zone::Battlefield);
    let nest = game.put(nest, P1, Zone::Battlefield);
    let curve = game.put(curve, P0, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: curve },
        &[Target::Object(brute)],
        &[],
    );
    assert_eq!(game.pt(brute), (5, 5), "one counter, doubled to two");
    game.combat(&[brute], &[(nest, brute)], &[], &[]);
    let insects = game
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            game.engine.state.objects[id].is_token && game.engine.state.objects[id].controller == P1
        })
        .count();
    assert_eq!(insects, 5, "a 5/5 deals all 5 to its blocker: five tokens");
}

#[test]
fn its_controller_creates_the_token() {
    let mut table = Table::default();
    let within = table.card(
        "{2}{G}",
        "Instant",
        None,
        "Destroy target permanent. Its controller creates a 3/3 green Bear creature token.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(3);
    let within = game.put(within, P0, Zone::Hand);
    let theirs = game.put(bear, P1, Zone::Battlefield);
    game.main();
    game.act(
        Action::Cast { object: within },
        &[Target::Object(theirs)],
        &[],
    );
    let p1_tokens: Vec<_> = tokens(&game)
        .into_iter()
        .filter(|id| game.engine.state.objects[id].controller == P1)
        .collect();
    assert_eq!(p1_tokens.len(), 1, "P1's Bear is replaced by P1's token");
    assert_eq!(game.pt(p1_tokens[0]), (3, 3));
    assert_eq!(tokens(&game).len(), 1, "none for the caster");
}

#[test]
fn a_countered_spells_controller_creates_treasures() {
    let mut table = Table::default();
    let offer = table.card(
        "{1}{U}",
        "Instant",
        None,
        "Counter target noncreature spell. Its controller creates two Treasure tokens.",
    );
    let heal = table.card("{W}", "Instant", None, "You gain 3 life.");
    let mut game = Game::new(table);
    game.lands(3);
    let offer = game.put(offer, P0, Zone::Hand);
    let heal = game.put(heal, P0, Zone::Hand);
    game.main();
    game.act_holding(Action::Cast { object: heal }, &[]);
    let spell = game.stack()[0];
    game.act(
        Action::Cast { object: offer },
        &[Target::Object(spell)],
        &[],
    );
    assert_eq!(game.life(P0), 20, "countered");
    assert_eq!(
        tokens(&game).len(),
        2,
        "two Treasures for the spell's controller"
    );
}
