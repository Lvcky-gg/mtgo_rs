//! Necropotence: "Skip your draw step. Whenever you discard a card, exile that card from
//! your graveyard. Pay 1 life: Exile the top card of your library face down. Put that card
//! into your hand at the beginning of your next end step."

use super::harness::*;
use mtg_core::{Step, Zone, ZoneRef};
use mtg_engine::actions::Action;

const NECRO: &str = "Skip your draw step.\nWhenever you discard a card, exile that card from \
                     your graveyard.\nPay 1 life: Exile the top card of your library face \
                     down. Put that card into your hand at the beginning of your next end step.";

fn exile(g: &Game) -> Vec<mtg_core::ObjectId> {
    g.engine.state.objects_in(ZoneRef::shared(Zone::Exile))
}

#[test]
fn paid_cards_wait_face_down_until_your_end_step() {
    let mut t = Table::default();
    let necro = t.card("{B}{B}{B}", "Enchantment", None, NECRO);
    let mut g = Game::new(t);
    let n = g.put(necro, P0, Zone::Battlefield);
    let actions = g.main();
    let hand = g.count(Zone::Hand, P0);
    let library = g.count(Zone::Library, P0);
    let pay = actions
        .iter()
        .find(|a| matches!(a, Action::ActivateAbility { source, .. } if *source == n))
        .cloned()
        .expect("pay 1 life is offered");
    g.act(pay.clone(), &[], &[]);
    g.main();
    g.act(pay, &[], &[]);
    assert_eq!(g.life(P0), 18);
    assert_eq!(g.count(Zone::Library, P0), library - 2);
    assert_eq!(g.count(Zone::Hand, P0), hand, "not yet");
    let exiled = exile(&g);
    assert_eq!(exiled.len(), 2);
    assert!(
        exiled.iter().all(|o| g.engine.state.objects[o].face_down),
        "exiled face down"
    );
    // At the beginning of this turn's end step, both come to hand.
    let c = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, c.id, mtg_engine::choice::Answer::Pass)
        .unwrap();
    g.drive(
        |_, c| match &c.kind {
            mtg_engine::choice::ChoiceKind::OrderTriggers { triggers, .. } => {
                Some(mtg_engine::choice::Answer::Order(triggers.clone()))
            }
            _ => None,
        },
        |g| g.engine.state.step == Step::End && g.stack().is_empty(),
    );
    assert_eq!(g.count(Zone::Hand, P0), hand + 2);
    assert!(exile(&g).is_empty());
}

#[test]
fn discarded_cards_are_exiled_and_the_draw_step_is_skipped() {
    let mut t = Table::default();
    let necro = t.card("{B}{B}{B}", "Enchantment", None, NECRO);
    let loot = t.card("{0}", "Sorcery", None, "Discard a card.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(necro, P0, Zone::Battlefield);
    let loot = g.put(loot, P0, Zone::Hand);
    g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(loot, &[]);
    assert_eq!(exile(&g).len(), 1, "the discarded card");
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "only the sorcery");
    let library = g.count(Zone::Library, P0);
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    assert_eq!(g.count(Zone::Library, P0), library, "no draw");
}
