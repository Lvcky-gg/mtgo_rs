//! Into the Flood Maw: "Gift a tapped Fish. Return target creature an opponent controls to
//! its owner's hand. If the gift was promised, instead return target nonland permanent an
//! opponent controls to its owner's hand."

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const MAW: &str = "Gift a tapped Fish (You may promise an opponent a gift as you cast this \
                   spell. If you do, they create a tapped 1/1 blue Fish creature token before \
                   its other effects.)\nReturn target creature an opponent controls to its \
                   owner's hand. If the gift was promised, instead return target nonland \
                   permanent an opponent controls to its owner's hand.";

/// Cast it choosing mode `mode`; the targets offered, and the game after (targeting the
/// artifact when it may, else the bear).
fn maw(mode: u8) -> (Game, Vec<Target>, mtg_core::ObjectId, mtg_core::ObjectId) {
    let mut t = Table::default();
    let maw = t.card("{U}", "Instant", None, MAW);
    let bear = t.bear();
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P1, Zone::Battlefield);
    let r = g.put(rock, P1, Zone::Battlefield);
    let maw = g.put(maw, P0, Zone::Hand);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, c.id, Answer::Action(Action::Cast { object: maw }))
        .unwrap();
    let offered = std::cell::RefCell::new(Vec::new());
    g.drive(
        |_, c| match &c.kind {
            ChoiceKind::ChooseModes { .. } if c.who == P0 => Some(Answer::Modes(vec![mode])),
            ChoiceKind::ChooseTargets { slots, .. } if c.who == P0 => {
                let slot = slots
                    .iter()
                    .find(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_default();
                *offered.borrow_mut() = slot.clone();
                let pick = if slot.contains(&Target::Object(r)) {
                    Target::Object(r)
                } else {
                    Target::Object(b)
                };
                Some(Answer::Targets(
                    slots
                        .iter()
                        .map(|s| if s.is_empty() { vec![] } else { vec![pick] })
                        .collect(),
                ))
            }
            _ => None,
        },
        |g| !offered.borrow().is_empty() && g.stack().is_empty(),
    );
    (g, offered.into_inner(), b, r)
}

#[test]
fn without_a_gift_only_creatures() {
    let (g, offered, b, r) = maw(0);
    assert!(offered.contains(&Target::Object(b)));
    assert!(!offered.contains(&Target::Object(r)));
    assert_eq!(g.count(Zone::Hand, P1), 1);
    let fish = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|o| g.engine.state.objects[o].is_token)
        .count();
    assert_eq!(fish, 0);
}

#[test]
fn with_a_gift_the_opponent_gets_a_tapped_fish_and_any_nonland_permanent_goes() {
    let (g, offered, _, r) = maw(1);
    assert!(offered.contains(&Target::Object(r)));
    assert!(
        !g.engine.state.objects.contains_key(&r),
        "the artifact was returned"
    );
    let fish: Vec<_> = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|o| g.engine.state.objects[o].is_token)
        .collect();
    assert_eq!(fish.len(), 1);
    let f = &g.engine.state.objects[&fish[0]];
    assert_eq!(f.controller, P1);
    assert!(f.tapped);
}
