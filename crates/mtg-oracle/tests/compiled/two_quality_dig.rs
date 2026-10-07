//! Each printed card quality gets its own optional quota from the same top group.
use super::{dig::stack_top, harness::*};
use mtg_core::{ObjectId, Zone, ZoneRef};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
};

fn choose_two(
    g: &mut Game,
    spell: ObjectId,
    offered: [Vec<ObjectId>; 2],
    picked: [Vec<ObjectId>; 2],
) {
    g.act_holding(Action::Cast { object: spell }, &[]);
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
                    return;
                }
                let answer = match &c.kind {
                    ChoiceKind::ChooseObjects { from, min, max } if asked < 2 => {
                        assert_eq!(from, &offered[asked]);
                        assert_eq!((*min, *max), (0, 1.min(from.len() as u32)));
                        if asked == 1 {
                            for id in &picked[0] {
                                assert_eq!(
                                    g.engine.state.objects[id].zone,
                                    ZoneRef::of(Zone::Library, P0),
                                    "choose both before moving either"
                                );
                            }
                        }
                        let answer = Answer::Objects(picked[asked].clone());
                        asked += 1;
                        answer
                    }
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                g.engine.answer(&g.table, c.id, answer).unwrap();
            }
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended"),
        }
    }
    panic!("selection did not finish");
}

fn cards_in(g: &Game, zone: Zone) -> Vec<mtg_core::CardId> {
    g.engine
        .state
        .objects_in(ZoneRef::of(zone, P0))
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect()
}

#[test]
fn creature_and_land_quotas_can_each_be_skipped_or_taken() {
    for take_creature in [false, true] {
        for take_land in [false, true] {
            let mut t = Table::default();
            let spell = t.card("{2}{G}", "Sorcery", None,
                "Reveal the top four cards of your library. You may put a creature card and/or a land card from among them into your hand. Put the rest into your graveyard.");
            let creature = t.bear();
            let land = t.card("", "Land", None, "");
            let rock = t.card("{2}", "Artifact", None, "");
            let mut g = Game::new(t);
            g.lands(3);
            let spell = g.put(spell, P0, Zone::Hand);
            g.main();
            let ids = stack_top(&mut g, &[creature, creature, land, rock]);
            choose_two(
                &mut g,
                spell,
                [ids[..2].to_vec(), vec![ids[2]]],
                [
                    if take_creature { vec![ids[0]] } else { vec![] },
                    if take_land { vec![ids[2]] } else { vec![] },
                ],
            );
            let hand = cards_in(&g, Zone::Hand);
            assert_eq!(hand.contains(&creature), take_creature);
            assert_eq!(hand.contains(&land), take_land);
            assert!(!hand.contains(&rock));
            assert_eq!(
                g.count(Zone::Graveyard, P0),
                5 - usize::from(take_creature) - usize::from(take_land)
            );
            assert_eq!(g.engine.state.revealed_cards.len(), 4);
        }
    }
}

#[test]
fn overlapping_qualities_cannot_select_one_card_twice() {
    for hybrid_first in [false, true] {
        let mut t = Table::default();
        let spell = t.card("{2}{G}", "Sorcery", None,
            "Reveal the top five cards of your library. You may put a creature card and/or an enchantment card from among them into your hand. Put the rest into your graveyard.");
        let hybrid = t.card("{G}", "Enchantment Creature — Bear", Some((1, 1)), "");
        let creature = t.bear();
        let enchantment = t.card("{G}", "Enchantment", None, "");
        let rock = t.card("{2}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(3);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        let ids = stack_top(&mut g, &[hybrid, creature, enchantment, rock, rock]);
        let (offered, picked) = if hybrid_first {
            // A malformed answer trying to choose the same card again is ignored.
            (vec![ids[2]], vec![ids[0], ids[2]])
        } else {
            (vec![ids[0], ids[2]], vec![ids[0]])
        };
        choose_two(
            &mut g,
            spell,
            [ids[..2].to_vec(), offered],
            [vec![ids[usize::from(!hybrid_first)]], picked],
        );
        let hand = cards_in(&g, Zone::Hand);
        assert!(hand.contains(&hybrid));
        assert_eq!(hand.contains(&enchantment), hybrid_first);
        assert_eq!(hand.contains(&creature), !hybrid_first);
        assert_eq!(g.count(Zone::Graveyard, P0), 4);
    }
}

#[test]
fn gift_of_the_gargantuan_reveals_only_selected_cards_and_bottoms_the_rest() {
    let mut t = Table::default();
    let spell = t.card("{2}{G}", "Sorcery", None,
        "Look at the top four cards of your library. You may reveal a creature card and/or a land card from among them and put the revealed cards into your hand. Put the rest on the bottom of your library in any order.");
    let creature = t.bear();
    let land = t.card("", "Land", None, "");
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[creature, land, rock, rock]);
    choose_two(
        &mut g,
        spell,
        [vec![ids[0]], vec![ids[1]]],
        [vec![ids[0]], vec![ids[1]]],
    );
    assert_eq!(g.engine.state.revealed_cards.len(), 2);
    let last_reveal = g
        .engine
        .log
        .iter()
        .rposition(|entry| matches!(entry.event, mtg_core::Event::Revealed { .. }))
        .unwrap();
    let first_move = g.engine.log.iter().position(|entry| matches!(entry.event,
        mtg_core::Event::ZoneChange { object, from, .. } if ids[..2].contains(&object) && from.zone == Zone::Library)).unwrap();
    assert!(
        last_reveal < first_move,
        "reveal both selected cards before moving either"
    );
    let lib = cards_in(&g, Zone::Library);
    assert_eq!(&lib[lib.len() - 2..], &[rock, rock]);
}

#[test]
fn absent_qualities_offer_empty_slots_and_move_the_whole_group_to_the_graveyard() {
    let mut t = Table::default();
    let spell = t.card("{2}{G}", "Sorcery", None,
        "Reveal the top four cards of your library. You may put a creature card and/or a land card from among them into your hand. Put the rest into your graveyard.");
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    stack_top(&mut g, &[rock, rock, rock, rock]);
    choose_two(&mut g, spell, [vec![], vec![]], [vec![], vec![]]);
    assert_eq!(
        cards_in(&g, Zone::Graveyard)
            .iter()
            .filter(|c| **c == rock)
            .count(),
        4
    );
}
