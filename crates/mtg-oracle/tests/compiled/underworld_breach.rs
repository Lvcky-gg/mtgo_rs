//! Underworld Breach: "Each nonland card in your graveyard has escape. The escape cost is
//! equal to the card's mana cost plus exile three other cards from your graveyard. At the
//! beginning of the end step, sacrifice this enchantment."

use super::harness::*;
use mtg_core::{Step, Zone, ZoneRef};
use mtg_engine::actions::Action;

const BREACH: &str = "Each nonland card in your graveyard has escape. The escape cost is equal \
                      to the card's mana cost plus exile three other cards from your \
                      graveyard. (You may cast cards from your graveyard for their escape \
                      cost.)\nAt the beginning of the end step, sacrifice this enchantment.";

fn castable(actions: &[Action], object: mtg_core::ObjectId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Cast { object: o } if *o == object))
}

#[test]
fn graveyard_cards_escape_for_their_cost_plus_three_others() {
    let mut t = Table::default();
    let breach = t.card("{1}{R}", "Enchantment", None, BREACH);
    let gift = t.card("{1}{R}", "Sorcery", None, "You gain 3 life.");
    let land = t.card("", "Land", None, "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    g.put(breach, P0, Zone::Battlefield);
    let gift = g.put(gift, P0, Zone::Graveyard);
    let dirt = g.put(land, P0, Zone::Graveyard);
    for _ in 0..5 {
        g.put(bear, P0, Zone::Graveyard);
    }
    let actions = g.main();
    assert!(castable(&actions, gift));
    assert!(!castable(&actions, dirt), "lands don't escape");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::PlayLand { object } if *object == dirt))
    );
    g.cast(gift, &[]);
    assert_eq!(g.life(P0), 23);
    let exiled = g
        .engine
        .state
        .objects_in(ZoneRef::shared(Zone::Exile))
        .len();
    assert_eq!(exiled, 3);
    // Back in the graveyard, it can escape again with the three cards that remain.
    let graveyard = g.engine.state.objects_in(ZoneRef::of(Zone::Graveyard, P0));
    assert_eq!(graveyard.len(), 4, "the sorcery and three others");
    let actions = g.main();
    let again = g
        .engine
        .state
        .objects_in(ZoneRef::of(Zone::Graveyard, P0))
        .into_iter()
        .find(|o| castable(&actions, *o))
        .expect("escapes again");
    g.cast(again, &[]);
    assert_eq!(g.life(P0), 26);
    // Nothing left to exile besides it: not enough.
    let actions = g.main();
    assert!(
        g.engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .into_iter()
            .all(|o| !castable(&actions, o))
    );
}

#[test]
fn only_its_controllers_graveyard_and_sacrificed_at_end_step() {
    let mut t = Table::default();
    let breach = t.card("{1}{R}", "Enchantment", None, BREACH);
    let gift = t.card("{0}", "Sorcery", None, "You gain 3 life.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(breach, P1, Zone::Battlefield);
    let gift = g.put(gift, P0, Zone::Graveyard);
    for _ in 0..3 {
        g.put(bear, P0, Zone::Graveyard);
    }
    let actions = g.main();
    assert!(!castable(&actions, gift), "the opponent's Breach");
    let c = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, c.id, mtg_engine::choice::Answer::Pass)
        .unwrap();
    g.drive(
        |_, _| None,
        |g| g.engine.state.step == Step::Cleanup || g.engine.state.active_player == P1,
    );
    assert!(g.find(breach).is_none(), "sacrificed at the end step");
}
