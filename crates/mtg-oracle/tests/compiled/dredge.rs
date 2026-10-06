//! Dredge (CR 702.52): a draw replaced by milling and returning the card from the graveyard.
use super::harness::*;
use mtg_core::{ObjectId, Step, Zone};
use mtg_engine::{
    Progress,
    choice::{Answer, ChoiceKind},
};

/// Play from the start of the turn into P0's main phase, answering the dredge question
/// with `dredge` and counting how often it was asked.
fn draw_step(g: &mut Game, dredge: Option<ObjectId>) -> usize {
    let mut asked = 0;
    loop {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended"),
            Progress::NeedsChoice(c) => {
                if let ChoiceKind::ChooseObjects { from, .. } = &c.kind
                    && c.because.contains("dredge")
                {
                    asked += 1;
                    assert_eq!(c.who, P0);
                    assert!(dredge.is_none_or(|d| from.contains(&d)));
                    let answer = Answer::Objects(dredge.into_iter().collect());
                    g.engine.answer(&g.table, c.id, answer).unwrap();
                    continue;
                }
                if matches!(c.kind, ChoiceKind::Priority { .. })
                    && g.engine.state.step == Step::PrecombatMain
                {
                    g.pending = Some(c);
                    return asked;
                }
                let a = c.default.clone().unwrap_or(Answer::Pass);
                g.engine.answer(&g.table, c.id, a).unwrap();
            }
        }
    }
}

fn dredger(t: &mut Table) -> mtg_core::CardId {
    t.card("{1}{B}", "Creature — Zombie", Some((2, 2)), "Dredge 2")
}

#[test]
fn the_draw_for_the_turn_can_be_dredged() {
    let mut t = Table::default();
    let d = dredger(&mut t);
    let mut g = Game::new(t);
    let card = g.put(d, P0, Zone::Graveyard);
    let library = g.count(Zone::Library, P0);
    assert_eq!(draw_step(&mut g, Some(card)), 1);
    let hand = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0));
    assert!(hand.iter().any(|id| g.engine.state.objects[id].card == d));
    assert_eq!(
        g.count(Zone::Hand, P0),
        1,
        "the dredged card instead of a draw"
    );
    assert_eq!(g.count(Zone::Graveyard, P0), 2, "two milled");
    assert_eq!(g.count(Zone::Library, P0), library - 2);
}

#[test]
fn declining_draws_as_usual() {
    let mut t = Table::default();
    let d = dredger(&mut t);
    let mut g = Game::new(t);
    g.put(d, P0, Zone::Graveyard);
    let library = g.count(Zone::Library, P0);
    assert_eq!(draw_step(&mut g, None), 1);
    assert_eq!(g.count(Zone::Hand, P0), 1);
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "still there");
    assert_eq!(g.count(Zone::Library, P0), library - 1);
}

#[test]
fn not_offered_without_enough_cards_in_the_library() {
    let mut t = Table::default();
    let d = dredger(&mut t);
    let mut g = Game::new(t);
    g.put(d, P0, Zone::Graveyard);
    let library = mtg_core::ZoneRef::of(Zone::Library, P0);
    while g.engine.state.objects_in(library).len() > 1 {
        let top = g.engine.state.objects_in(library)[0];
        g.engine.state.objects.remove(&top);
        if let Some(order) = g.engine.state.zone_order.get_mut(&library) {
            order.retain(|o| *o != top);
        }
    }
    assert_eq!(draw_step(&mut g, None), 0);
    assert_eq!(g.count(Zone::Hand, P0), 1);
}

#[test]
fn a_spell_draw_can_be_dredged_one_card_at_a_time() {
    let mut t = Table::default();
    let d = dredger(&mut t);
    let divination = t.card("{2}{U}", "Sorcery", None, "Draw two cards.");
    let mut g = Game::new(t);
    g.lands(3);
    let card = g.put(d, P0, Zone::Graveyard);
    let s = g.put(divination, P0, Zone::Hand);
    draw_step(&mut g, None);
    let hand = g.count(Zone::Hand, P0);
    // The first draw is dredged; the dredged card is then in hand, so the second is not.
    g.act(
        mtg_engine::actions::Action::Cast { object: s },
        &[],
        &[Answer::Objects(vec![card])],
    );
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand - 1 + 2,
        "spell gone; dredged card and a draw"
    );
    assert_eq!(g.count(Zone::Graveyard, P0), 3, "two milled and the spell");
}
