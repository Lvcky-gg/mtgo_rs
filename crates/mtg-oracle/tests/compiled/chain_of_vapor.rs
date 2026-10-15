//! Chain of Vapor: "Return target nonland permanent to its owner's hand. Then that
//! permanent's controller may sacrifice a land of their choice. If the player does, they may
//! copy this spell and may choose a new target for that copy."

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const CHAIN: &str = "Return target nonland permanent to its owner's hand. Then that \
                     permanent's controller may sacrifice a land of their choice. If the player \
                     does, they may copy this spell and may choose a new target for that copy.";

fn chain(opponent_copies: bool) -> (Game, mtg_core::ObjectId, mtg_core::ObjectId) {
    let mut t = Table::default();
    let chain = t.card("{0}", "Instant", None, CHAIN);
    let bear = t.bear();
    let land = t.card("", "Land", None, "");
    let mut g = Game::new(t);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let their_land = g.put(land, P1, Zone::Battlefield);
    let chain = g.put(chain, P0, Zone::Hand);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: chain }),
        )
        .unwrap();
    let stage = std::cell::Cell::new(0);
    g.drive(
        |_, c| match &c.kind {
            ChoiceKind::ChooseTargets { .. } if c.who == P0 => {
                Some(Answer::Targets(vec![vec![Target::Object(theirs)]]))
            }
            ChoiceKind::ChooseTargets { .. } if c.who == P1 => {
                Some(Answer::Targets(vec![vec![Target::Object(mine)]]))
            }
            ChoiceKind::ChooseObjects { .. } if c.who == P1 => {
                stage.set(1);
                Some(Answer::Objects(if opponent_copies {
                    vec![their_land]
                } else {
                    vec![]
                }))
            }
            ChoiceKind::Confirm if c.who == P1 => Some(Answer::Bool(true)),
            _ => None,
        },
        |g| stage.get() == 1 && g.stack().is_empty(),
    );
    (g, mine, their_land)
}

#[test]
fn the_opponent_can_sacrifice_a_land_to_chain_it_back() {
    let (g, mine, land) = chain(true);
    assert_eq!(g.count(Zone::Hand, P1), 1, "their bear");
    assert!(
        !g.engine.state.objects.contains_key(&mine),
        "copy bounced mine"
    );
    assert!(
        !g.engine.state.objects.contains_key(&land),
        "land sacrificed"
    );
}

#[test]
fn declining_ends_the_chain() {
    let (g, mine, land) = chain(false);
    assert_eq!(g.count(Zone::Hand, P1), 1);
    assert!(g.engine.state.objects.contains_key(&mine));
    assert!(g.engine.state.objects.contains_key(&land));
}
