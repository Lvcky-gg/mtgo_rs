//! Mockingbird: "You may have this creature enter as a copy of any creature on the
//! battlefield with mana value less than or equal to the amount of mana spent to cast this
//! creature, except it's a Bird in addition to its other types and it has flying."

use super::harness::*;
use mtg_core::{Keyword, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const MOCKINGBIRD: &str = "Flying\nYou may have this creature enter as a copy of any creature \
                           on the battlefield with mana value less than or equal to the amount \
                           of mana spent to cast this creature, except it's a Bird in addition \
                           to its other types and it has flying.";

#[test]
fn copies_only_what_the_mana_spent_allows_and_becomes_a_flying_bird() {
    let mut t = Table::default();
    let bird = t.card("{X}{U}", "Creature — Bird Bard", Some((1, 1)), MOCKINGBIRD);
    let ogre = t.card("{2}{G}", "Creature — Bear", Some((3, 3)), "");
    let giant = t.card("{3}{G}", "Creature — Bear", Some((5, 5)), "");
    let mut g = Game::new(t);
    g.lands(3);
    let o = g.put(ogre, P1, Zone::Battlefield);
    let gi = g.put(giant, P1, Zone::Battlefield);
    let bird = g.put(bird, P0, Zone::Hand);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: bird }),
        )
        .unwrap();
    let offered = std::cell::RefCell::new(None);
    g.drive(
        |_, c| match &c.kind {
            ChoiceKind::ChooseX { .. } => Some(Answer::Number(2)),
            ChoiceKind::ChooseObjects { from, .. } if c.who == P0 => {
                *offered.borrow_mut() = Some(from.clone());
                Some(Answer::Objects(vec![o]))
            }
            _ => None,
        },
        |g| offered.borrow().is_some() && g.stack().is_empty(),
    );
    let offered = offered.into_inner().expect("asked what to copy");
    assert!(offered.contains(&o));
    assert!(!offered.contains(&gi), "mana value 4 > 3 spent");
    let copy = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| {
            g.engine.state.objects[id].controller == P0
                && *id != o
                && *id != gi
                && !g.engine.state.objects[id].tapped
        })
        .expect("the copy");
    assert_eq!(g.pt(copy), (3, 3));
    assert!(g.has(copy, Keyword::Flying));
    let bird_type = mtg_ir::PrintedCards::subtype_named(&g.table, "Bird").unwrap();
    let ch = g.engine.characteristics(&g.table, copy).unwrap().clone();
    assert!(ch.subtypes.contains(&bird_type));
}
