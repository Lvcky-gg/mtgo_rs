//! Optional choices allow no targets; required choices reject empty answers.
use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
};

#[test]
fn printed_optional_and_required_targets_are_reported_and_enforced() {
    for optional in [false, true] {
        let mut t = Table::default();
        let spell = t.card(
            "{U}",
            "Sorcery",
            None,
            if optional {
                "Return up to one target creature to its owner's hand."
            } else {
                "Return target creature to its owner's hand."
            },
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let creature = g.put(bear, P1, Zone::Battlefield);
        let object = g.put(spell, P0, Zone::Hand);
        g.main();
        let priority = g.pending.take().unwrap();
        g.engine
            .answer(
                &g.table,
                priority.id,
                Answer::Action(Action::Cast { object }),
            )
            .unwrap();
        let mut asked = false;
        for _ in 0..1_000 {
            match g.engine.advance(&g.table) {
                Progress::NeedsChoice(c) => {
                    if g.stack().is_empty()
                        && matches!(c.kind, ChoiceKind::Priority { .. })
                        && asked
                    {
                        break;
                    }
                    let answer = match &c.kind {
                        ChoiceKind::ChooseTargets {
                            slots,
                            optional: metadata,
                        } => {
                            assert_eq!(metadata, &[optional]);
                            assert_eq!(slots, &[vec![Target::Object(creature)]]);
                            assert!(
                                matches!(&c.default, Some(Answer::Targets(picks)) if picks == &vec![vec![Target::Object(creature)]])
                            );
                            asked = true;
                            if optional {
                                Answer::Targets(vec![vec![]])
                            } else {
                                // CR 601.2c: the player must choose the required target.
                                // An invalid selection cannot silently choose one for them.
                                assert!(
                                    g.engine
                                        .answer(&g.table, c.id, Answer::Targets(vec![vec![]]))
                                        .is_err()
                                );
                                let Progress::NeedsChoice(preserved) = g.engine.advance(&g.table)
                                else {
                                    panic!("rejected target selection must preserve the question");
                                };
                                assert_eq!(preserved.id, c.id);
                                assert!(matches!(preserved.kind,
                                    ChoiceKind::ChooseTargets { slots: ref kept, optional: ref flags }
                                    if kept == slots && flags == metadata));
                                Answer::Targets(vec![vec![Target::Object(creature)]])
                            }
                        }
                        _ => c.default.clone().unwrap_or(Answer::Pass),
                    };
                    g.engine.answer(&g.table, c.id, answer).unwrap();
                }
                Progress::Continue => {}
                Progress::GameOver { .. } => panic!("game ended"),
            }
        }
        assert!(asked);
        assert_eq!(
            g.find(bear).is_some(),
            optional,
            "required target is explicitly chosen after rejection; optional one stays empty"
        );
        assert_eq!(g.count(Zone::Graveyard, P0), 1);
        assert_eq!(g.count(Zone::Hand, P1), usize::from(!optional));
    }
}

#[test]
fn an_any_number_run_can_stop_before_or_after_its_first_target() {
    for take in 0..=1 {
        let mut t = Table::default();
        let spell = t.card(
            "{U}",
            "Sorcery",
            None,
            "Return any number of target creatures to their owners' hands.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        g.put(bear, P1, Zone::Battlefield);
        g.put(bear, P1, Zone::Battlefield);
        let object = g.put(spell, P0, Zone::Hand);
        g.main();
        let priority = g.pending.take().unwrap();
        g.engine
            .answer(
                &g.table,
                priority.id,
                Answer::Action(Action::Cast { object }),
            )
            .unwrap();
        let mut prompts = 0;
        for _ in 0..1_000 {
            match g.engine.advance(&g.table) {
                Progress::NeedsChoice(c) => {
                    if g.stack().is_empty()
                        && matches!(c.kind, ChoiceKind::Priority { .. })
                        && prompts > 0
                    {
                        break;
                    }
                    let answer = match &c.kind {
                        ChoiceKind::ChooseTargets { slots, optional } => {
                            assert_eq!(optional, &[true]);
                            assert!(matches!(&c.default, Some(Answer::Targets(picks))
                                if picks[0].is_empty() == (prompts > 0)));
                            let chosen = if prompts < take {
                                vec![slots[0][0]]
                            } else {
                                vec![]
                            };
                            prompts += 1;
                            Answer::Targets(vec![chosen])
                        }
                        _ => c.default.clone().unwrap_or(Answer::Pass),
                    };
                    g.engine.answer(&g.table, c.id, answer).unwrap();
                }
                Progress::Continue => {}
                Progress::GameOver { .. } => panic!("game ended"),
            }
        }
        assert_eq!(
            prompts,
            take + 1,
            "declining ends the run instead of asking the remaining slots"
        );
        assert_eq!(g.count(Zone::Hand, P1), take);
        assert_eq!(
            g.engine
                .state
                .battlefield()
                .iter()
                .filter(|id| g.engine.state.objects[id].card == bear)
                .count(),
            2 - take
        );
        assert_eq!(g.count(Zone::Graveyard, P0), 1);
    }
}
