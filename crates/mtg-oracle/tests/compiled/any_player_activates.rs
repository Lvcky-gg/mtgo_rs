//! "Any player may activate this ability." (CR 602.1b): the activator pays for it and
//! controls it, so "you" is them.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

fn setup(text: &str) -> (Game, mtg_core::ObjectId) {
    let mut table = Table::default();
    let source = table.card("{2}", "Artifact", None, text);
    let land = table.card("", "Land", None, "{T}: Add one mana of any color.");
    let mut game = Game::new(table);
    let source = game.put(source, P0, Zone::Battlefield);
    game.put(land, P1, Zone::Battlefield);
    game.main();
    game.until(P1, mtg_core::Step::PrecombatMain);
    (game, source)
}

#[test]
fn an_opponent_activates_it_and_draws() {
    let (mut game, source) = setup("{1}: Draw a card. Any player may activate this ability.");
    let actions = game.until(P1, mtg_core::Step::PrecombatMain);
    let activate = actions
        .iter()
        .find(|a| matches!(a, Action::ActivateAbility { source: s, .. } if *s == source))
        .cloned()
        .expect("offered to P1");
    let (mine, theirs) = (game.count(Zone::Hand, P0), game.count(Zone::Hand, P1));
    let log_len = game.engine.log.len();
    let c = game.pending.take().unwrap();
    game.engine
        .answer(
            &game.table,
            c.id,
            mtg_engine::choice::Answer::Action(activate),
        )
        .unwrap();
    game.until(P0, mtg_core::Step::PrecombatMain);
    // P1 paid, from P1's land.
    assert!(
        game.engine.log[log_len..]
            .iter()
            .any(|e| matches!(e.event, mtg_core::Event::ManaSpent { player: P1, .. }))
    );
    assert_eq!(game.count(Zone::Hand, P1), theirs + 1, "P1 drew");
    assert!(
        game.count(Zone::Hand, P0) <= mine + 1,
        "only P0's own draw step"
    );
}

#[test]
fn an_ordinary_ability_is_not_offered_to_an_opponent() {
    let (mut game, source) = setup("{1}: Draw a card.");
    let actions = game.until(P1, mtg_core::Step::PrecombatMain);
    assert!(!offers(&actions, source));
}
