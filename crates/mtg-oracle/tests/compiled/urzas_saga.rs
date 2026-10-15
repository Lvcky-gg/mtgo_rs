//! Urza's Saga: a land Saga whose first two chapters grant it abilities and whose third
//! fetches an artifact with mana cost {0} or {1}.

use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const SAGA: &str = "(As this Saga enters and after your draw step, add a lore counter. \
                    Sacrifice after III.)\nI — This Saga gains \"{T}: Add {C}.\"\nII — This \
                    Saga gains \"{2}, {T}: Create a 0/0 colorless Construct artifact creature \
                    token with 'This token gets +1/+1 for each artifact you control.'\"\nIII \
                    — Search your library for an artifact card with mana cost {0} or {1}, put \
                    it onto the battlefield, then shuffle.";

#[test]
fn chapters_grant_mana_then_constructs_then_fetch_a_cheap_artifact() {
    let mut t = Table::default();
    let saga = t.named_card(
        "Urza's Saga",
        "",
        "Enchantment Land — Urza's Saga",
        None,
        SAGA,
    );
    let zero = t.card("{0}", "Artifact", None, "");
    let one = t.card("{1}", "Artifact", None, "");
    let blue = t.card("{U}", "Artifact", None, "");
    let two = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let library: Vec<_> = [zero, one, blue, two]
        .into_iter()
        .map(|c| g.put(c, P0, Zone::Library))
        .collect();
    let s = g.put(saga, P0, Zone::Hand);
    g.main();
    g.act(Action::PlayLand { object: s }, &[], &[]);
    let s = g.find(saga).expect("on the battlefield");

    // Chapter I: it taps for {C}.
    let mana = mtg_engine::mana::mana_sources(&g.engine.state, &g.table, P0);
    assert!(
        mana.iter().any(|m| m.object == s),
        "a mana source after chapter I"
    );

    // Chapter II, after the next draw step: {2}, {T} makes a Construct.
    g.until(P1, Step::PrecombatMain);
    let actions = g.until(P0, Step::PrecombatMain);
    let make = actions
        .iter()
        .find(|a| matches!(a, Action::ActivateAbility { source, .. } if *source == s))
        .cloned()
        .expect("chapter II's ability is offered");
    g.act(make, &[], &[]);
    let constructs: Vec<_> = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].is_token)
        .collect();
    assert_eq!(constructs.len(), 1);
    let construct = constructs[0];
    assert_eq!(g.pt(construct), (1, 1), "it counts itself");

    // Chapter III: only exact {0} / {1} costs are offered; then the Saga is sacrificed.
    g.until(P1, Step::PrecombatMain);
    let c = g.pending.take().unwrap();
    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
    let mut offered = None;
    g.drive(
        |_, c| match &c.kind {
            ChoiceKind::ChooseObjects { from, .. } if c.who == P0 => {
                offered = Some(from.clone());
                Some(Answer::Objects(vec![library[1]]))
            }
            _ => None,
        },
        |g| {
            g.engine.state.active_player == P0
                && g.engine.state.step == Step::PrecombatMain
                && g.stack().is_empty()
        },
    );
    let mut offered = offered.expect("chapter III searched");
    offered.sort();
    assert_eq!(offered, vec![library[0], library[1]]);
    assert!(
        g.find(one).is_some(),
        "the {{1}} artifact was put onto the battlefield"
    );
    assert!(g.find(saga).is_none(), "sacrificed after III");
    assert_eq!(g.pt(construct), (2, 2));
}
