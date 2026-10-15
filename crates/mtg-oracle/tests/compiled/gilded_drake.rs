//! Gilded Drake: "When this creature enters, exchange control of this creature and up to
//! one target creature an opponent controls. If you don't or can't make an exchange,
//! sacrifice this creature. This ability still resolves if its target becomes illegal."

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

const DRAKE: &str = "Flying\nWhen this creature enters, exchange control of this creature and \
                     up to one target creature an opponent controls. If you don't or can't \
                     make an exchange, sacrifice this creature. This ability still resolves if \
                     its target becomes illegal.";

fn controller(g: &Game, id: mtg_core::ObjectId) -> mtg_core::PlayerId {
    mtg_engine::layers::controller(&g.engine.state, id).unwrap()
}

#[test]
fn swaps_for_an_opposing_creature() {
    let mut t = Table::default();
    let drake = t.card("{0}", "Creature — Drake", Some((3, 3)), DRAKE);
    let bear = t.bear();
    let mut g = Game::new(t);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let drake_card = drake;
    let drake = g.put(drake, P0, Zone::Hand);
    g.main();
    g.cast(drake, &[Target::Object(theirs)]);
    let d = g.find(drake_card).expect("on the battlefield");
    assert_eq!(controller(&g, d), P1);
    assert_eq!(controller(&g, theirs), P0);
}

#[test]
fn with_nothing_to_take_it_is_sacrificed() {
    let mut t = Table::default();
    let drake = t.card("{0}", "Creature — Drake", Some((3, 3)), DRAKE);
    let mut g = Game::new(t);
    let drake_card = drake;
    let drake = g.put(drake, P0, Zone::Hand);
    g.main();
    g.cast(drake, &[]);
    assert!(g.find(drake_card).is_none());
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn a_target_removed_in_response_still_means_a_sacrifice() {
    let mut t = Table::default();
    let drake = t.card("{0}", "Creature — Drake", Some((3, 3)), DRAKE);
    let kill = t.card("{0}", "Instant", None, "Destroy target creature.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let drake_card = drake;
    let drake = g.put(drake, P0, Zone::Hand);
    let kill = g.put(kill, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: drake }, &[]);
    // The Drake resolves; its trigger goes on the stack targeting the bear.
    g.act_holding(Action::Pass, &[Target::Object(theirs)]);
    g.act(
        Action::Cast { object: kill },
        &[Target::Object(theirs)],
        &[],
    );
    assert!(g.find(drake_card).is_none(), "sacrificed, not fizzled");
}
