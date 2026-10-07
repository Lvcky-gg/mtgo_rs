use super::harness::*;
use mtg_core::{Color, ManaPool, Zone, ZoneRef};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
};

#[test]
fn mana_cost_announcements_are_hidden_without_hiding_real_stack_objects() {
    for storage in [false, true] {
        for holding_spell in [false, true] {
            let mut t = Table::default();
            let source_card = t.card("{1}", "Artifact", None,
                if storage {
                    "{T}, Remove any number of charge counters from this artifact: Add {C} for each charge counter removed this way."
                } else {
                    "{T}, Tap an untapped creature you control: Add one mana of any color."
                });
            let spell_card = t.card("{0}", "Instant", None, "You gain 1 life.");
            let creature = t.bear();
            let mut g = Game::new(t);
            let source = g.put(source_card, P0, Zone::Battlefield);
            let helper = g.put(creature, P0, Zone::Battlefield);
            let spell = g.put(spell_card, P0, Zone::Hand);
            let ability = mtg_engine::abilities::current(&g.engine.state, &g.table, source)
                .into_iter()
                .find(|a| {
                    matches!(
                        a.kind,
                        mtg_ir::AbilityKind::Activated {
                            is_mana_ability: true,
                            ..
                        }
                    )
                })
                .unwrap();
            let counter = match &ability.kind {
                mtg_ir::AbilityKind::Activated { cost, .. } => {
                    cost.additional.iter().find_map(|part| match part {
                        mtg_ir::AdditionalCost::RemoveCounters { kind, .. } => Some(*kind),
                        _ => None,
                    })
                }
                _ => None,
            };
            let ability_id = ability.id;
            if let Some(kind) = counter {
                g.engine
                    .state
                    .objects
                    .get_mut(&source)
                    .unwrap()
                    .counters
                    .insert(kind, 3);
            }
            g.main();
            if holding_spell {
                g.act_holding(Action::Cast { object: spell }, &[]);
            }
            let before = mtg_engine::view::project(&g.engine.state, P0).stack;
            assert_eq!(before.len(), usize::from(holding_spell));
            let c = g.pending.take().unwrap();
            g.engine
                .answer(
                    &g.table,
                    c.id,
                    Answer::Action(Action::ActivateManaAbility {
                        source,
                        ability: ability_id,
                        color: if storage { None } else { Some(Color::Green) },
                    }),
                )
                .unwrap();
            let mut answered = false;
            for _ in 0..1000 {
                match g.engine.advance(&g.table) {
                    Progress::Continue => {}
                    Progress::NeedsChoice(c) => {
                        let answer = if storage {
                            assert!(matches!(c.kind, ChoiceKind::ChooseX { min: 0, max: 3 }));
                            Answer::Number(2)
                        } else {
                            assert!(
                                matches!(&c.kind, ChoiceKind::ChooseObjects { from, min: 1, max: 1 }
                                if from == &vec![helper])
                            );
                            Answer::Objects(vec![helper])
                        };
                        let internal: Vec<_> = g
                            .engine
                            .state
                            .objects_in(ZoneRef::shared(Zone::Stack))
                            .into_iter()
                            .filter(|id| !before.contains(id))
                            .collect();
                        assert_eq!(internal.len(), 1);
                        for viewer in [P0, P1] {
                            let view = mtg_engine::view::project_showing(
                                &g.engine.state,
                                viewer,
                                &internal,
                            );
                            assert_eq!(view.stack, before);
                            assert!(view.visible.contains_key(&source));
                            assert!(view.visible.contains_key(&helper));
                            assert!(!view.visible.contains_key(&internal[0]));
                            for id in &before {
                                assert!(view.visible.contains_key(id));
                            }
                        }
                        g.engine.answer(&g.table, c.id, answer).unwrap();
                        answered = true;
                        break;
                    }
                    Progress::GameOver { .. } => panic!("game ended"),
                }
            }
            assert!(answered);
            let mut resolved = false;
            for _ in 0..1000 {
                match g.engine.advance(&g.table) {
                    Progress::Continue => {}
                    Progress::NeedsChoice(c) => {
                        assert!(matches!(c.kind, ChoiceKind::Priority { .. }));
                        assert_eq!(
                            g.engine.state.objects_in(ZoneRef::shared(Zone::Stack)),
                            before
                        );
                        let mana = &g.engine.state.players[&P0].mana.amounts;
                        assert_eq!(
                            mana[if storage {
                                ManaPool::COLORLESS_SLOT
                            } else {
                                Color::Green as usize
                            }],
                            if storage { 2 } else { 1 }
                        );
                        resolved = true;
                        break;
                    }
                    Progress::GameOver { .. } => panic!("game ended"),
                }
            }
            assert!(resolved);
        }
    }
}
