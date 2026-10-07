use super::harness::*;
use mtg_core::{CardId, PlayerId, Target, Zone, ZoneRef};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
};

fn library(g: &Game, who: PlayerId) -> Vec<CardId> {
    g.engine
        .state
        .objects_in(ZoneRef::of(Zone::Library, who))
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect()
}

#[test]
fn plural_placement_rejects_unimplemented_destinations() {
    struct NoSubtypes;
    impl mtg_oracle::convert::Subtypes for NoSubtypes {
        fn intern(&self, _: &str) -> Option<u16> {
            None
        }
    }
    for text in [
        "Put two target creature cards from a graveyard on top of your library.",
        "Put two target lands into their owners' libraries second from the top.",
    ] {
        let result = mtg_oracle::compile::compile(
            &mtg_oracle::compile::FaceText {
                name: "Placement",
                card_types: &[mtg_core::CardType::Sorcery],
                subtypes: &[],
                oracle_text: Some(text),
                mana_cost: "{G}",
            },
            &NoSubtypes,
        );
        assert!(!result.understood(), "{text}");
    }
}

#[test]
fn selected_graveyard_cards_are_ordered_before_the_followup_draw() {
    for count in [0, 1, 3] {
        let mut t = Table::default();
        let spell = t.card("{1}{B}", "Instant", None,
            "Put any number of target creature cards from your graveyard on top of your library.\nDraw a card.");
        let creatures: Vec<_> = (0..3).map(|_| t.bear()).collect();
        let mut g = Game::new(t);
        g.lands(2);
        let ids: Vec<_> = creatures
            .iter()
            .map(|card| g.put(*card, P0, Zone::Graveyard))
            .collect();
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        let before = library(&g, P0);
        let hand = g.count(Zone::Hand, P0);
        let targets: Vec<_> = ids[..count].iter().copied().map(Target::Object).collect();
        let answers = if count == 3 {
            vec![Answer::Objects(vec![ids[2]]), Answer::Objects(vec![ids[0]])]
        } else {
            vec![]
        };
        let c = g.pending.take().unwrap();
        g.engine
            .answer(
                &g.table,
                c.id,
                Answer::Action(Action::Cast { object: spell }),
            )
            .unwrap();
        let mut targets = targets.into_iter();
        let mut answers = answers.into_iter();
        let mut resolved = false;
        for _ in 0..1000 {
            match g.engine.advance(&g.table) {
                Progress::Continue => {}
                Progress::NeedsChoice(c) => {
                    if matches!(c.kind, ChoiceKind::Priority { .. })
                        && c.who == P0
                        && g.stack().is_empty()
                    {
                        g.pending = Some(c);
                        resolved = true;
                        break;
                    }
                    let answer = match &c.kind {
                        ChoiceKind::ChooseTargets { optional, .. } => {
                            let picked = targets.next();
                            assert!(picked.is_some() || optional == &[true]);
                            Answer::Targets(vec![picked.into_iter().collect()])
                        }
                        ChoiceKind::Priority { .. } => Answer::Pass,
                        _ => answers.next().or(c.default).expect("answer"),
                    };
                    g.engine.answer(&g.table, c.id, answer).unwrap();
                }
                Progress::GameOver { .. } => panic!("game ended"),
            }
        }
        assert!(resolved);
        assert_eq!(g.count(Zone::Hand, P0), hand);
        let after = library(&g, P0);
        match count {
            0 => assert_eq!(after, before[1..]),
            1 => {
                assert_eq!(after, before);
                assert!(
                    g.engine
                        .state
                        .objects_in(ZoneRef::of(Zone::Hand, P0))
                        .iter()
                        .any(|id| g.engine.state.objects[id].card == creatures[0])
                );
            }
            3 => {
                assert_eq!(&after[..2], &[creatures[0], creatures[1]]);
                assert_eq!(&after[2..], before);
                assert!(
                    g.engine
                        .state
                        .objects_in(ZoneRef::of(Zone::Hand, P0))
                        .iter()
                        .any(|id| g.engine.state.objects[id].card == creatures[2])
                );
            }
            _ => unreachable!(),
        }
        assert_eq!(g.count(Zone::Graveyard, P0), 4 - count);
    }
}

#[test]
fn bottom_placement_preserves_the_chosen_order_below_existing_cards() {
    let mut t = Table::default();
    let spell = t.card(
        "{B}",
        "Sorcery",
        None,
        "Put two target creature cards from your graveyard on the bottom of your library.",
    );
    let first_card = t.bear();
    let second_card = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let first = g.put(first_card, P0, Zone::Graveyard);
    let second = g.put(second_card, P0, Zone::Graveyard);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let before = library(&g, P0);
    g.act(
        Action::Cast { object: spell },
        &[Target::Object(first), Target::Object(second)],
        &[Answer::Objects(vec![second])],
    );
    let after = library(&g, P0);
    assert_eq!(&after[..before.len()], before);
    assert_eq!(&after[before.len()..], &[second_card, first_card]);
}

#[test]
fn opposing_owner_chooses_the_order_for_their_library() {
    let mut t = Table::default();
    let spell = t.card(
        "{3}{G}{G}",
        "Sorcery",
        None,
        "Put two target lands on top of their owners' libraries.",
    );
    let first_card = t.mountain();
    let second_card = t.card("", "Land — Forest", None, "");
    let mut g = Game::new(t);
    g.lands(5);
    let first = g.put(first_card, P1, Zone::Battlefield);
    let second = g.put(second_card, P1, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let before = library(&g, P1);
    g.act_holding(
        Action::Cast { object: spell },
        &[Target::Object(first), Target::Object(second)],
    );
    let c = g.pending.take().unwrap();
    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
    let mut ordered = false;
    for _ in 0..1000 {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => {
                if let ChoiceKind::ChooseObjects { from, min, max } = &c.kind {
                    assert_eq!(c.who, P1);
                    assert_eq!((*min, *max), (1, 1));
                    assert_eq!(from, &vec![first, second]);
                    g.engine
                        .answer(&g.table, c.id, Answer::Objects(vec![second]))
                        .unwrap();
                    ordered = true;
                    break;
                }
                g.engine
                    .answer(&g.table, c.id, c.default.unwrap_or(Answer::Pass))
                    .unwrap();
            }
            Progress::GameOver { .. } => panic!("game ended"),
        }
    }
    assert!(ordered);
    g.main();
    let after = library(&g, P1);
    assert_eq!(&after[..2], &[second_card, first_card]);
    assert_eq!(&after[2..], before);
}

#[test]
fn cards_with_different_owners_go_to_their_respective_libraries() {
    let mut t = Table::default();
    let spell = t.card(
        "{3}{G}{G}",
        "Sorcery",
        None,
        "Put two target lands on top of their owners' libraries.",
    );
    let land = t.mountain();
    let mut g = Game::new(t);
    g.lands(5);
    let a = g.put(land, P0, Zone::Battlefield);
    let b = g.put(land, P1, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let before = [library(&g, P0), library(&g, P1)];
    g.cast(spell, &[Target::Object(a), Target::Object(b)]);
    for (i, who) in [P0, P1].into_iter().enumerate() {
        let after = library(&g, who);
        assert_eq!(after[0], land);
        assert_eq!(&after[1..], before[i]);
    }
}
