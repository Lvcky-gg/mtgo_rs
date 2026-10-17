//! "One or two target creatures": at least one, at most two.

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};

const SPELL: &str = "One or two target creatures each get +2/+0 until end of turn.";

#[test]
fn one_target_is_enough_and_two_both_get_the_bonus() {
    for picks in [1usize, 2] {
        let mut t = Table::default();
        let spell = t.card("{R}", "Instant", None, SPELL);
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let a = g.put(bear, P0, Zone::Battlefield);
        let b = g.put(bear, P0, Zone::Battlefield);
        let s = g.put(spell, P0, Zone::Hand);
        g.main();
        let c = g.pending.take().unwrap();
        g.engine
            .answer(&g.table, c.id, Answer::Action(Action::Cast { object: s }))
            .unwrap();
        // Each slot in turn: a creature, or (the optional second, with one pick) none.
        let mut slot = 0;
        g.drive(
            |_, c| match c.kind {
                mtg_engine::ChoiceKind::ChooseTargets { .. } => {
                    let pick = [a, b].get(slot).filter(|_| slot < picks).copied();
                    slot += 1;
                    Some(Answer::Targets(vec![
                        pick.map(Target::Object).into_iter().collect(),
                    ]))
                }
                _ => None,
            },
            |g| g.stack().is_empty(),
        );
        assert_eq!(g.pt(a), (4, 2));
        assert_eq!(g.pt(b), if picks == 2 { (4, 2) } else { (2, 2) });
    }
}

#[test]
fn with_no_creature_at_all_it_cannot_be_cast() {
    let mut t = Table::default();
    let spell = t.card("{R}", "Instant", None, SPELL);
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(spell, P0, Zone::Hand);
    assert!(!g.main().contains(&Action::Cast { object: s }));
}
