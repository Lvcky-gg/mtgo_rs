//! Top-card ordering belongs to the resolving controller, including opposing libraries.
use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
};

#[test]
fn opposing_library_is_ordered_by_caster_and_visible_only_to_caster_during_choice() {
    let mut t = Table::default();
    let spell = t.card("{U}", "Sorcery", None,
        "Look at the top three cards of target opponent's library, then put them back in any order.");
    let bear = t.bear();
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let lib = ZoneRef::of(Zone::Library, P1);
    let ids: Vec<_> = [bear, rock, bear, rock]
        .into_iter()
        .map(|card| g.put(card, P1, Zone::Library))
        .collect();
    let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
    order.retain(|id| !ids.contains(id));
    for (i, id) in ids.iter().enumerate() {
        order.insert(i, *id);
    }
    g.act_holding(Action::Cast { object: spell }, &[Target::Player(P1)]);
    let priority = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, priority.id, Answer::Pass)
        .unwrap();
    let mut asked = 0;
    for _ in 0..1000 {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) => {
                if g.stack().is_empty() && matches!(c.kind, ChoiceKind::Priority { .. }) {
                    assert_eq!(asked, 2);
                    g.pending = Some(c);
                    break;
                }
                let answer = if let ChoiceKind::ChooseObjects { from, min, max } = &c.kind {
                    assert_eq!(c.who, P0);
                    assert_eq!((*min, *max), (1, 1));
                    assert!(
                        from.iter()
                            .all(|id| g.engine.view_for(P0).visible.contains_key(id))
                    );
                    assert!(
                        from.iter()
                            .all(|id| !g.engine.view_for(P1).visible.contains_key(id))
                    );
                    assert!(!g.engine.view_for(P0).visible.contains_key(&ids[3]));
                    let pick = [ids[2], ids[0]][asked];
                    assert!(from.contains(&pick));
                    asked += 1;
                    Answer::Objects(vec![pick])
                } else {
                    c.default.clone().unwrap_or(Answer::Pass)
                };
                g.engine.answer(&g.table, c.id, answer).unwrap();
            }
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended"),
        }
    }
    let lib = g.engine.state.objects_in(lib);
    assert_eq!(lib[3], ids[3]);
    assert_eq!(
        lib[..3]
            .iter()
            .map(|id| g.engine.state.objects[id].card)
            .collect::<Vec<_>>(),
        vec![bear, bear, rock]
    );
    assert!(g.engine.state.revealed_cards.is_empty());
    assert!(!g.engine.view_for(P0).visible.contains_key(&lib[0]));
}

#[test]
fn inline_hand_count_controls_how_many_cards_are_ordered() {
    let mut t = Table::default();
    let source_card = t.card("{U}", "Creature — Bear", Some((1,1)),
        "{1}{U}: Look at the top X cards of your library, where X is the number of cards in your hand, then put them back in any order.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let source = g.put(source_card, P0, Zone::Battlefield);
    let legal = g.main();
    let hand = ZoneRef::of(Zone::Hand, P0);
    for id in g.engine.state.objects_in(hand) {
        g.engine.state.objects.remove(&id);
    }
    g.engine.state.zone_order.remove(&hand);
    g.put(bear, P0, Zone::Hand);
    g.put(bear, P0, Zone::Hand);
    let ids = super::dig::stack_top(&mut g, &[bear, bear, bear]);
    let action = legal
        .into_iter()
        .find(|a| matches!(a,Action::ActivateAbility {source:s,..} if *s==source))
        .unwrap();
    super::dig::resolve_selection(&mut g, action, &ids[..2], 1, 1, vec![ids[1]]);
    assert_eq!(
        g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[2],
        ids[2]
    );
}

#[test]
fn undefined_x_reorder_stays_rejected_but_mana_x_reorder_compiles() {
    use mtg_oracle::compile::{FaceText, SubtypeNames, compile};
    let text = "Look at the top X cards of your library, then put them back in any order.";
    for (cost, expected) in [("{U}", false), ("{X}{U}", true)] {
        let face = FaceText {
            name: "Reorder",
            card_types: &[mtg_core::CardType::Sorcery],
            subtypes: &[],
            oracle_text: Some(text),
            mana_cost: cost,
        };
        assert_eq!(compile(&face, &SubtypeNames(vec![])).understood(), expected);
    }
}
