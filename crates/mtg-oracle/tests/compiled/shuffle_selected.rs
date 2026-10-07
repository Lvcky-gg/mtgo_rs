use super::harness::*;
use mtg_core::{Event, ObjectId, Target, Zone, ZoneRef};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
};

// Explicitly decline unused optional targets and reject any ordering prompts.
fn cast_without_ordering(g: &mut Game, spell: ObjectId, targets: &[Target]) {
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let mut targets = targets.iter().copied();
    for _ in 0..1000 {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => {
                if matches!(c.kind, ChoiceKind::Priority { .. })
                    && c.who == P0
                    && g.stack().is_empty()
                {
                    g.pending = Some(c);
                    return;
                }
                let answer = match c.kind {
                    ChoiceKind::ChooseTargets { optional, .. } => {
                        let picked = targets.next();
                        assert!(picked.is_some() || optional == [true]);
                        Answer::Targets(vec![picked.into_iter().collect()])
                    }
                    ChoiceKind::Priority { .. } => Answer::Pass,
                    _ => panic!("unexpected resolution choice: {}", c.because),
                };
                g.engine.answer(&g.table, c.id, answer).unwrap();
            }
            Progress::GameOver { .. } => panic!("game ended"),
        }
    }
    panic!("spell never resolved");
}

#[test]
fn selected_graveyard_cards_are_shuffled_without_an_ordering_choice() {
    for count in [0, 1, 3] {
        let mut t = Table::default();
        let spell_card = t.card(
            "{G}",
            "Sorcery",
            None,
            "Shuffle any number of target creature cards from your graveyard into your library.",
        );
        let creature = t.bear();
        let land = t.mountain();
        let mut g = Game::new(t);
        g.lands(1);
        let ids: Vec<_> = (0..3)
            .map(|_| g.put(creature, P0, Zone::Graveyard))
            .collect();
        let own_land = g.put(land, P0, Zone::Graveyard);
        let opponent_card = g.put(creature, P1, Zone::Graveyard);
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.main();
        let before = g.count(Zone::Library, P0);
        let targets: Vec<_> = ids[..count].iter().copied().map(Target::Object).collect();
        cast_without_ordering(&mut g, spell, &targets);
        assert_eq!(g.count(Zone::Library, P0), before + count);
        assert_eq!(g.count(Zone::Graveyard, P0), 5 - count);
        assert!(g.engine.state.objects.contains_key(&own_land));
        assert!(g.engine.state.objects.contains_key(&opponent_card));
        for (i, id) in ids.into_iter().enumerate() {
            assert_eq!(g.engine.state.objects.contains_key(&id), i >= count);
        }
        let creatures_in_library = g
            .engine
            .state
            .objects_in(ZoneRef::of(Zone::Library, P0))
            .iter()
            .filter(|id| g.engine.state.objects[id].card == creature)
            .count();
        assert_eq!(creatures_in_library, count);
        assert!(
            g.engine
                .log
                .iter()
                .any(|e| matches!(&e.event, Event::Shuffled { player, .. } if *player == P0))
        );
    }
}

#[test]
fn optional_single_card_shuffle_accepts_noncreature_cards() {
    let mut t = Table::default();
    let spell_card = t.card(
        "{G}",
        "Sorcery",
        None,
        "Shuffle up to one target card from your graveyard into your library.",
    );
    let land = t.mountain();
    let mut g = Game::new(t);
    g.lands(1);
    let land = g.put(land, P0, Zone::Graveyard);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    let before = g.count(Zone::Library, P0);
    cast_without_ordering(&mut g, spell, &[Target::Object(land)]);
    assert_eq!(g.count(Zone::Library, P0), before + 1);
    assert!(!g.engine.state.objects.contains_key(&land));
}

#[test]
fn shuffle_into_your_library_rejects_cards_from_other_graveyards() {
    struct NoSubtypes;
    impl mtg_oracle::convert::Subtypes for NoSubtypes {
        fn intern(&self, _: &str) -> Option<u16> {
            None
        }
    }
    let result = mtg_oracle::compile::compile(
        &mtg_oracle::compile::FaceText {
            name: "Shuffle",
            card_types: &[mtg_core::CardType::Sorcery],
            subtypes: &[],
            oracle_text: Some(
                "Shuffle any number of target creature cards from a graveyard into your library.",
            ),
            mana_cost: "{G}",
        },
        &NoSubtypes,
    );
    assert!(!result.understood());
}

#[test]
fn whole_graveyard_shuffles_do_not_ask_for_intermediate_ordering() {
    let mut t = Table::default();
    let spell_card = t.card(
        "{U}",
        "Sorcery",
        None,
        "Target player shuffles their graveyard into their library.",
    );
    let creature = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    for _ in 0..3 {
        g.put(creature, P1, Zone::Graveyard);
    }
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    let before = g.count(Zone::Library, P1);
    cast_without_ordering(&mut g, spell, &[Target::Player(P1)]);
    assert_eq!(g.count(Zone::Graveyard, P1), 0);
    assert_eq!(g.count(Zone::Library, P1), before + 3);
    assert!(
        g.engine
            .log
            .iter()
            .any(|e| matches!(&e.event, Event::Shuffled { player, .. } if *player == P1))
    );
}
