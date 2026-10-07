//! Coin flips (CR 705) and clashes (CR 701.23).

use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer, state::Rng};

/// Put a card with this mana cost on top of `who`'s library.
fn top_card(game: &mut Game, card: mtg_core::CardId, who: mtg_core::PlayerId) {
    let id = game.put(card, who, Zone::Library);
    let library = game
        .engine
        .state
        .zone_order
        .get_mut(&ZoneRef::of(Zone::Library, who))
        .unwrap();
    library.retain(|o| *o != id);
    library.insert(0, id);
}

fn clash_game(mine: &str, theirs: &str) -> (Game, mtg_core::ObjectId, mtg_core::CardId) {
    let mut table = Table::default();
    let spell = table.card(
        "{U}",
        "Sorcery",
        None,
        "Draw a card. Clash with an opponent. If you win, return ~ to its owner's hand.",
    );
    let my_top = table.card(mine, "Sorcery", None, "Draw a card.");
    let their_top = table.card(theirs, "Sorcery", None, "Draw a card.");
    let mut game = Game::new(table);
    game.lands(1);
    let spell = game.put(spell, P0, Zone::Hand);
    game.main();
    // After the draw for the turn: the spell's own draw takes the top card, so the clash
    // reveals the one under it.
    top_card(&mut game, my_top, P0);
    top_card(&mut game, my_top, P0);
    top_card(&mut game, their_top, P1);
    (game, spell, their_top)
}

#[test]
fn winning_a_clash_returns_the_spell_to_hand() {
    let (mut game, spell, their_top) = clash_game("{4}{U}", "{U}");
    let hand = game.count(Zone::Hand, P0);
    game.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Bool(true), Answer::Bool(false)],
    );
    // Cast (-1), drew (+1), and returned (+1).
    assert_eq!(game.count(Zone::Hand, P0), hand + 1);
    assert_eq!(
        game.count(Zone::Graveyard, P0),
        0,
        "back in hand, not in the graveyard"
    );
    assert!(game.engine.log.iter().any(|e| e.event
        == mtg_core::Event::Clashed {
            player: P0,
            won: true
        }));
    // The opponent chose the bottom for their revealed card.
    let their_library = game.engine.state.objects_in(ZoneRef::of(Zone::Library, P1));
    let bottom = their_library.last().unwrap();
    assert_eq!(game.engine.state.objects[bottom].card, their_top);
}

#[test]
fn losing_a_clash_leaves_the_spell_in_the_graveyard() {
    let (mut game, spell, _) = clash_game("{U}", "{4}{U}");
    let hand = game.count(Zone::Hand, P0);
    game.act(Action::Cast { object: spell }, &[], &[]);
    assert_eq!(game.count(Zone::Hand, P0), hand);
    assert_eq!(game.count(Zone::Graveyard, P0), 1);
}

#[test]
fn a_coin_flip_does_what_its_result_says() {
    let mut outcomes = std::collections::BTreeSet::new();
    for seed in 0u8..16 {
        let mut table = Table::default();
        let spell = table.card(
            "{R}",
            "Sorcery",
            None,
            "Flip a coin. If you win the flip, you gain 5 life. If you lose the flip, you lose \
             3 life.",
        );
        let mut game = Game::new(table);
        game.lands(1);
        let id = game.put(spell, P0, Zone::Hand);
        game.engine.state.rng = Rng::from_seed(&[seed; 32]);
        game.main();
        game.act(Action::Cast { object: id }, &[], &[]);
        let won = game
            .engine
            .log
            .iter()
            .find_map(|e| match e.event {
                mtg_core::Event::CoinFlipped { player: P0, won } => Some(won),
                _ => None,
            })
            .expect("a flip");
        assert_eq!(game.life(P0), if won { 25 } else { 17 });
        outcomes.insert(won);
    }
    assert_eq!(outcomes.len(), 2, "both outcomes come up");
}
