//! Library searches: to the top, by name, onto the battlefield.
use super::harness::*;
use mtg_core::{Event, Zone, ZoneRef};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
};

#[test]
fn cultivate_splits_the_searched_lands_and_reveals_both_before_moving_them() {
    for put_second_on_board in [false, true] {
        let mut t = Table::default();
        let tutor = t.card("{2}{G}", "Sorcery", None,
            "Search your library for up to two basic land cards, reveal those cards, put one onto the battlefield tapped and the other into your hand, then shuffle.");
        let forest = t.card("", "Basic Land — Forest", None, "");
        let island = t.card("", "Basic Land — Island", None, "");
        let mut g = Game::new(t);
        g.lands(3);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let first = g.put(forest, P0, Zone::Library);
        let second = g.put(island, P0, Zone::Library);
        let (board, hand) = if put_second_on_board {
            (island, forest)
        } else {
            (forest, island)
        };
        let pick = if put_second_on_board { second } else { first };
        g.act(
            Action::Cast { object: spell },
            &[],
            &[
                Answer::Objects(vec![first, second]),
                Answer::Objects(vec![pick]),
            ],
        );
        let found = g.find(board).expect("chosen land enters the battlefield");
        assert!(g.engine.state.objects[&found].tapped);
        assert!(g.find(hand).is_none());
        assert!(
            g.engine
                .state
                .objects_in(ZoneRef::of(Zone::Hand, P0))
                .iter()
                .any(|id| g.engine.state.objects[id].card == hand)
        );
        assert_eq!(g.engine.state.revealed_cards.len(), 2);
        let last_reveal = g
            .engine
            .log
            .iter()
            .rposition(|entry| matches!(entry.event, Event::Revealed { .. }))
            .unwrap();
        let first_move = g.engine.log.iter().position(|entry| matches!(entry.event,
            Event::ZoneChange { object, from, .. } if (object == first || object == second) && from.zone == Zone::Library)).unwrap();
        assert!(last_reveal < first_move);
    }
}

#[test]
fn cultivate_can_find_zero_or_one_land_and_one_land_must_enter_the_battlefield() {
    for number_found in 0..=1 {
        let mut t = Table::default();
        let tutor = t.card("{2}{G}", "Sorcery", None,
            "Search your library for up to two basic land cards, reveal those cards, put one onto the battlefield tapped and the other into your hand, then shuffle.");
        let forest = t.card("", "Basic Land — Forest", None, "");
        let mut g = Game::new(t);
        g.lands(3);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let selected: Vec<_> = (0..number_found)
            .map(|_| g.put(forest, P0, Zone::Library))
            .collect();
        let hand_before = g.count(Zone::Hand, P0);
        // A malformed empty second answer must not skip the mandatory land drop.
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Objects(selected), Answer::Objects(vec![])],
        );
        assert_eq!(g.find(forest).is_some(), number_found == 1);
        assert_eq!(g.count(Zone::Hand, P0), hand_before - 1);
        assert_eq!(g.engine.state.revealed_cards.len(), number_found);
        assert!(
            g.engine
                .log
                .iter()
                .any(|entry| matches!(entry.event, Event::Shuffled { player: P0 }))
        );
    }
}

#[test]
fn split_tutors_cannot_put_an_unsearched_land_onto_the_board() {
    let mut t = Table::default();
    let tutor = t.card("{2}{G}", "Sorcery", None,
        "Search your library for up to two basic land cards, reveal those cards, put one onto the battlefield tapped and the other into your hand, then shuffle.");
    let forest = t.card("", "Basic Land — Forest", None, "");
    let island = t.card("", "Basic Land — Island", None, "");
    let mountain = t.card("", "Basic Land — Mountain", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    let spell = g.put(tutor, P0, Zone::Hand);
    g.main();
    let first = g.put(forest, P0, Zone::Library);
    let second = g.put(island, P0, Zone::Library);
    let unrelated = g.put(mountain, P0, Zone::Library);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[
            Answer::Objects(vec![first, first, second]),
            Answer::Objects(vec![unrelated, unrelated]),
        ],
    );
    assert!(g.find(mountain).is_none());
    assert_eq!(g.engine.state.objects[&unrelated].zone.zone, Zone::Library);
    assert!(g.find(forest).is_some() || g.find(island).is_some());
    let in_hand = g
        .engine
        .state
        .objects_in(ZoneRef::of(Zone::Hand, P0))
        .iter()
        .filter(|id| [forest, island].contains(&g.engine.state.objects[id].card))
        .count();
    assert_eq!(in_hand, 1);
    assert_eq!(g.engine.state.revealed_cards.len(), 2);
}

#[test]
fn a_sacrificed_tutor_source_still_splits_the_searched_lands() {
    let mut t = Table::default();
    let tutor = t.card("{2}{G}", "Creature — Bear", Some((2, 2)),
        "{2}, {T}, Sacrifice ~: Search your library for up to two basic land cards, reveal them, put one onto the battlefield tapped and the other into your hand, then shuffle.");
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let source = g.put(tutor, P0, Zone::Battlefield);
    g.main();
    let first = g.put(forest, P0, Zone::Library);
    let second = g.put(forest, P0, Zone::Library);
    g.act(
        activate(source, 0),
        &[],
        &[
            Answer::Objects(vec![first, second]),
            Answer::Objects(vec![first]),
        ],
    );
    assert!(g.find(tutor).is_none());
    assert!(
        g.engine
            .state
            .objects
            .values()
            .any(|o| o.card == tutor && o.zone.zone == Zone::Graveyard)
    );
    let land = g.find(forest).unwrap();
    assert!(g.engine.state.objects[&land].tapped);
    assert!(
        g.engine
            .state
            .objects
            .values()
            .any(|o| o.card == forest && o.zone.zone == Zone::Hand)
    );
}

#[test]
fn navigation_orb_accepts_basic_lands_or_nonbasic_gates_but_not_other_nonbasic_lands() {
    for (kind, eligible) in [
        ("Basic Land — Forest", true),
        ("Land — Gate", true),
        ("Land — Forest Island", false),
    ] {
        let mut t = Table::default();
        let orb = t.card("{3}", "Artifact", None,
            "{2}, {T}, Sacrifice ~: Search your library for up to two basic land cards and/or Gate cards, reveal those cards, put one onto the battlefield tapped and the other into your hand, then shuffle.");
        let land = t.card("", kind, None, "");
        let mut g = Game::new(t);
        g.lands(2);
        let source = g.put(orb, P0, Zone::Battlefield);
        g.main();
        let selected = g.put(land, P0, Zone::Library);
        g.act(
            activate(source, 0),
            &[],
            &[
                Answer::Objects(vec![selected]),
                Answer::Objects(vec![selected]),
            ],
        );
        assert_eq!(g.find(land).is_some(), eligible, "{kind}");
        assert_eq!(g.engine.state.revealed_cards.len(), usize::from(eligible));
        if let Some(found) = g.find(land) {
            assert!(g.engine.state.objects[&found].tapped);
        }
    }
}

#[test]
fn a_search_and_reveal_sentence_preserves_the_found_cards_for_a_later_split() {
    let mut t = Table::default();
    let tutor = t.card("{2}{G}", "Sorcery", None,
        "Search your library for up to two Forest cards and reveal them. Put one of them onto the battlefield tapped and the other into your hand, then shuffle.");
    let forest = t.card("", "Basic Land — Forest", None, "");
    let dual = t.card("", "Land — Forest Island", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    let source = g.put(tutor, P0, Zone::Hand);
    g.main();
    let first = g.put(forest, P0, Zone::Library);
    let second = g.put(dual, P0, Zone::Library);
    g.act(
        Action::Cast { object: source },
        &[],
        &[
            Answer::Objects(vec![first, second]),
            Answer::Objects(vec![second]),
        ],
    );
    assert!(g.engine.state.objects[&g.find(dual).unwrap()].tapped);
    assert!(
        g.engine
            .state
            .objects
            .values()
            .any(|o| o.card == forest && o.zone.zone == Zone::Hand)
    );
    assert_eq!(g.engine.state.revealed_cards.len(), 2);
}

#[test]
fn route_searches_put_both_basic_and_nonbasic_subtype_lands_onto_the_battlefield() {
    for subtype in ["Gate", "Desert"] {
        let mut t = Table::default();
        let tutor = t.card("{3}{G}", "Sorcery", None, &format!(
            "Search your library for up to two basic land cards and/or {subtype} cards, put them onto the battlefield tapped, then shuffle."));
        let forest = t.card("", "Basic Land — Forest", None, "");
        let special = t.card("", &format!("Land — {subtype}"), None, "");
        let mut g = Game::new(t);
        g.lands(4);
        let source = g.put(tutor, P0, Zone::Hand);
        g.main();
        let first = g.put(forest, P0, Zone::Library);
        let second = g.put(special, P0, Zone::Library);
        g.act(
            Action::Cast { object: source },
            &[],
            &[Answer::Objects(vec![first, second])],
        );
        for card in [forest, special] {
            assert!(g.engine.state.objects[&g.find(card).unwrap()].tapped);
        }
        assert!(g.engine.state.revealed_cards.is_empty());
    }
}

#[test]
fn farseek_accepts_each_listed_subtype_and_nonbasic_duals_but_not_plain_forests() {
    for (kind, eligible) in [
        ("Basic Land — Plains", true),
        ("Basic Land — Island", true),
        ("Basic Land — Swamp", true),
        ("Basic Land — Mountain", true),
        ("Land — Forest Island", true),
        ("Basic Land — Forest", false),
        ("Artifact", false),
    ] {
        let mut t = Table::default();
        let tutor = t.card("{1}{G}", "Sorcery", None,
            "Search your library for a Plains, Island, Swamp, or Mountain card, put it onto the battlefield tapped, then shuffle.");
        let land = t.card("", kind, None, "");
        let mut g = Game::new(t);
        g.lands(2);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let selected = g.put(land, P0, Zone::Library);
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Objects(vec![selected])],
        );
        assert_eq!(g.find(land).is_some(), eligible, "{kind}");
        if let Some(found) = g.find(land) {
            assert!(g.engine.state.objects[&found].tapped);
        }
        assert!(
            g.engine
                .log
                .iter()
                .any(|entry| matches!(entry.event, Event::Shuffled { player: P0 }))
        );
    }
}

#[test]
fn chord_of_calling_uses_the_announced_x_as_its_creature_mana_value_limit() {
    for (x, cost, kind, eligible) in [
        (0, "{0}", "Creature — Bear", true),
        (1, "{G}", "Creature — Bear", true),
        (1, "{1}{G}", "Creature — Bear", false),
        (3, "{2}{G}", "Creature — Bear", true),
        (3, "{G}", "Artifact", false),
    ] {
        let mut t = Table::default();
        let tutor = t.card("{X}{G}{G}{G}", "Instant", None,
            "Convoke\nSearch your library for a creature card with mana value X or less, put it onto the battlefield, then shuffle.");
        let card = t.card(
            cost,
            kind,
            kind.starts_with("Creature").then_some((1, 1)),
            "",
        );
        let mut g = Game::new(t);
        g.lands(x as usize + 3);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let selected = g.put(card, P0, Zone::Library);
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Number(x), Answer::Objects(vec![selected])],
        );
        assert_eq!(g.find(card).is_some(), eligible, "X={x}, {cost}, {kind}");
        if let Some(found) = g.find(card) {
            assert!(!g.engine.state.objects[&found].tapped);
        }
    }
}

#[test]
fn green_suns_zenith_filters_color_and_shuffles_itself_back_even_when_no_card_is_found() {
    for (cost, eligible) in [("{G}", true), ("{B}", false), ("{1}{G}", false)] {
        let mut t = Table::default();
        let tutor = t.card("{X}{G}", "Sorcery", None,
            "Search your library for a green creature card with mana value X or less, put it onto the battlefield, then shuffle. Shuffle ~ into its owner's library.");
        let creature = t.card(cost, "Creature — Bear", Some((1, 1)), "");
        let mut g = Game::new(t);
        g.lands(2);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let selected = g.put(creature, P0, Zone::Library);
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Number(1), Answer::Objects(vec![selected])],
        );
        assert_eq!(g.find(creature).is_some(), eligible);
        assert!(
            g.engine
                .state
                .objects_in(ZoneRef::of(Zone::Library, P0))
                .iter()
                .any(|id| g.engine.state.objects[id].card == tutor)
        );
        assert!(
            !g.engine
                .state
                .objects_in(ZoneRef::of(Zone::Graveyard, P0))
                .iter()
                .any(|id| g.engine.state.objects[id].card == tutor)
        );
    }
}

#[test]
fn citanul_flute_reveals_and_puts_a_creature_within_activated_x_into_hand() {
    for (cost, eligible) in [("{1}{G}", true), ("{2}{G}", false)] {
        let mut t = Table::default();
        let flute = t.card("{5}", "Artifact", None,
            "{X}, {T}: Search your library for a creature card with mana value X or less, reveal it, put it into your hand, then shuffle.");
        let creature = t.card(cost, "Creature — Bear", Some((1, 1)), "");
        let mut g = Game::new(t);
        g.lands(2);
        let source = g.put(flute, P0, Zone::Battlefield);
        g.main();
        let selected = g.put(creature, P0, Zone::Library);
        g.act(
            activate(source, 0),
            &[],
            &[Answer::Number(2), Answer::Objects(vec![selected])],
        );
        assert!(g.engine.state.objects[&source].tapped);
        let found = g
            .engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == creature);
        assert_eq!(found, eligible);
        assert_eq!(g.engine.state.revealed_cards.len(), usize::from(eligible));
    }
}

#[test]
fn dynamic_quality_filters_observe_equal_lower_and_upper_bounds() {
    for (quality, x, expected) in [
        ("power X or less", 0, [true, false, false]),
        ("power X or greater", 2, [false, true, true]),
        ("toughness X", 2, [false, true, false]),
        ("toughness X or greater", 0, [true, true, true]),
        ("toughness X or greater", 2, [false, true, true]),
        ("mana value X", 2, [false, true, false]),
    ] {
        let mut t = Table::default();
        let spell = t.card(
            "{X}{B}",
            "Sorcery",
            None,
            &format!("Destroy all creatures with {quality}."),
        );
        let cards = [
            t.card("{G}", "Creature — Bear", Some((0, 1)), ""),
            t.card("{1}{G}", "Creature — Bear", Some((2, 2)), ""),
            t.card("{2}{G}", "Creature — Bear", Some((3, 3)), ""),
        ];
        let mut g = Game::new(t);
        g.lands(x as usize + 1);
        for card in cards {
            g.put(card, P1, Zone::Battlefield);
        }
        let source = g.put(spell, P0, Zone::Hand);
        g.main();
        g.act(Action::Cast { object: source }, &[], &[Answer::Number(x)]);
        for (card, destroyed) in cards.into_iter().zip(expected) {
            assert_eq!(
                g.find(card).is_none(),
                destroyed,
                "{quality}, X={x}, {card:?}"
            );
        }
    }
}

#[test]
fn quality_filters_reject_x_when_the_spell_does_not_define_it() {
    use mtg_oracle::compile::{FaceText, SubtypeNames, compile};
    for text in [
        "Destroy all creatures with power X or less.",
        "Destroy all creatures with toughness X or greater.",
        "Search your library for a creature card with mana value X or less, put it into your hand, then shuffle.",
    ] {
        let face = FaceText {
            name: "Undefined X",
            card_types: &[mtg_core::CardType::Sorcery],
            subtypes: &[],
            oracle_text: Some(text),
            mana_cost: "{B}",
        };
        assert!(
            !compile(&face, &SubtypeNames(vec![])).understood(),
            "{text}"
        );
    }
}

#[test]
fn noun_lists_do_not_consume_commas_between_separate_effects() {
    let mut t = Table::default();
    let lord = t.card("{1}{U}{B}{R}", "Legendary Creature — Zombie", Some((10, 4)),
        "When ~ enters, you lose 2 life, you sacrifice two creatures, and target opponent draws two cards.\n{B}: Regenerate ~.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    let first = g.put(bear, P0, Zone::Battlefield);
    let second = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(lord, P0, Zone::Hand);
    g.main();
    let hand_before = g.count(Zone::Hand, P1);
    g.act(
        Action::Cast { object: spell },
        &[mtg_core::Target::Player(P1)],
        &[Answer::Objects(vec![first, second])],
    );
    assert_eq!(g.life(P0), 18);
    assert_eq!(g.count(Zone::Hand, P1), hand_before + 2);
    assert!(g.find(bear).is_none());
    assert!(g.find(lord).is_some());
}

#[test]
fn a_tutor_puts_the_card_in_hand_before_randomly_discarding() {
    let mut t = Table::default();
    let tutor = t.card("{R}", "Sorcery", None,
        "Search your library for a card, put that card into your hand, discard a card at random, then shuffle.");
    let marker = t.card("{5}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(tutor, P0, Zone::Hand);
    g.main();
    let selected = g.put(marker, P0, Zone::Library);
    let hand = g.engine.state.objects_in(ZoneRef::of(Zone::Hand, P0));
    for card in hand.into_iter().filter(|id| *id != spell) {
        g.engine.state.objects.get_mut(&card).unwrap().zone = ZoneRef::of(Zone::Exile, P0);
    }
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![selected])],
    );
    assert_eq!(g.count(Zone::Hand, P0), 0);
    let graveyard = g.engine.state.objects_in(ZoneRef::of(Zone::Graveyard, P0));
    assert!(
        graveyard
            .iter()
            .any(|id| g.engine.state.objects[id].card == marker),
        "the tutored card must enter the hand and then be the only possible random discard"
    );
    let (move_index, in_hand) = g
        .engine
        .log
        .iter()
        .enumerate()
        .find_map(|(i, entry)| match entry.event {
            Event::ZoneChange {
                object,
                new_object,
                from,
                to,
                ..
            } if object == selected && from.zone == Zone::Library && to.zone == Zone::Hand => {
                Some((i, new_object))
            }
            _ => None,
        })
        .expect("selected card moves from library to hand");
    let discard_index = g.engine.log.iter().position(|entry| matches!(entry.event,
        Event::ZoneChange { object, from, to, .. } if object == in_hand && from.zone == Zone::Hand && to.zone == Zone::Graveyard))
        .expect("selected card is discarded from hand");
    let shuffle_index = g
        .engine
        .log
        .iter()
        .position(|entry| matches!(entry.event, Event::Shuffled { player: P0 }))
        .unwrap();
    assert!(move_index < discard_index && discard_index < shuffle_index);
}

#[test]
fn unrestricted_tutors_require_a_card_but_filtered_and_optional_searches_can_find_none() {
    for (text, min) in [
        (
            "Search your library for a card, put that card into your hand, then shuffle.",
            1,
        ),
        (
            "Search your library for a creature card, reveal it, put it into your hand, then shuffle.",
            0,
        ),
        (
            "Search your library for up to one card, put it into your hand, then shuffle.",
            0,
        ),
    ] {
        let mut t = Table::default();
        let tutor = t.card("{B}", "Sorcery", None, text);
        let marker = t.card("{5}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        g.put(marker, P0, Zone::Library);
        let hand_before = g.count(Zone::Hand, P0);
        let library_before = g.count(Zone::Library, P0);
        let priority = g.pending.take().unwrap();
        g.engine
            .answer(
                &g.table,
                priority.id,
                Answer::Action(Action::Cast { object: spell }),
            )
            .unwrap();
        let choice = loop {
            match g.engine.advance(&g.table) {
                Progress::Continue => {}
                Progress::NeedsChoice(choice)
                    if matches!(choice.kind, ChoiceKind::ChooseObjects { .. }) =>
                {
                    break choice;
                }
                Progress::NeedsChoice(choice) => {
                    g.engine.answer(&g.table, choice.id, Answer::Pass).unwrap();
                }
                Progress::GameOver { .. } => panic!("tutor should ask for a selection"),
            }
        };
        assert!(
            matches!(choice.kind, ChoiceKind::ChooseObjects { min: actual, max: 1, .. } if actual == min),
            "{text}: {:?}",
            choice.kind
        );
        let ChoiceKind::ChooseObjects { from, .. } = &choice.kind else {
            unreachable!()
        };
        let searching = g.engine.view_for(P0);
        let opponent = g.engine.view_for(P1);
        for id in from {
            assert!(
                searching
                    .visible
                    .get(id)
                    .is_some_and(|card| card.card.is_some()),
                "the picker must disclose searchable cards to the searching player"
            );
            assert!(
                !opponent.visible.contains_key(id),
                "searching must not reveal the library to the opponent"
            );
        }
        g.engine
            .answer(&g.table, choice.id, Answer::Objects(vec![]))
            .unwrap();
        // The resolver sanitizes malformed mandatory answers by filling their
        // minimum. An empty answer can never make an unrestricted tutor fizzle.
        loop {
            match g.engine.advance(&g.table) {
                Progress::Continue => {}
                Progress::NeedsChoice(choice)
                    if matches!(choice.kind, ChoiceKind::Priority { .. }) =>
                {
                    break;
                }
                other => panic!("unexpected progress after selecting: {other:?}"),
            }
        }
        assert_eq!(g.count(Zone::Hand, P0), hand_before - 1 + min as usize);
        assert_eq!(g.count(Zone::Library, P0), library_before - min as usize);
    }
}

#[test]
fn a_random_discard_tutor_with_an_empty_library_still_discards() {
    let mut t = Table::default();
    let tutor = t.card("{R}", "Sorcery", None,
        "Search your library for a card, put that card into your hand, discard a card at random, then shuffle.");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(tutor, P0, Zone::Hand);
    g.main();
    let library = ZoneRef::of(Zone::Library, P0);
    for id in g.engine.state.objects_in(library) {
        g.engine.state.objects.remove(&id);
    }
    g.engine.state.zone_order.get_mut(&library).unwrap().clear();
    let hand_before = g.count(Zone::Hand, P0);
    g.act(Action::Cast { object: spell }, &[], &[]);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand_before - 2,
        "cast the tutor and discard one other card"
    );
    assert_eq!(g.count(Zone::Graveyard, P0), 2);
    assert!(
        g.engine
            .log
            .iter()
            .any(|entry| matches!(entry.event, Event::Shuffled { player: P0 }))
    );
}

#[test]
fn random_tutor_discard_can_hit_the_found_card_or_an_existing_hand_card() {
    let mut outcomes = std::collections::BTreeSet::new();
    for seed in 0..32u8 {
        let run = || {
            let mut t = Table::default();
            let tutor = t.card("{R}", "Sorcery", None,
                "Search your library for a card, put that card into your hand, discard a card at random, then shuffle.");
            let marker = t.card("{5}", "Artifact", None, "");
            let other = t.card("{2}", "Enchantment", None, "");
            let mut g = Game::new(t);
            g.lands(1);
            let spell = g.put(tutor, P0, Zone::Hand);
            g.put(other, P0, Zone::Hand);
            g.main();
            let pick = g.put(marker, P0, Zone::Library);
            let hand_before = g.count(Zone::Hand, P0);
            let library_before = g.count(Zone::Library, P0);
            g.engine.state.rng = mtg_engine::state::Rng::from_seed(&[seed; 32]);
            g.act(
                Action::Cast { object: spell },
                &[],
                &[Answer::Objects(vec![pick])],
            );
            assert_eq!(g.count(Zone::Hand, P0), hand_before - 1);
            assert_eq!(g.count(Zone::Library, P0), library_before - 1);
            let discards: Vec<_> = g
                .engine
                .log
                .iter()
                .filter_map(|entry| match entry.event {
                    Event::ZoneChange {
                        new_object,
                        from,
                        to,
                        ..
                    } if from.zone == Zone::Hand && to.zone == Zone::Graveyard => {
                        Some(g.engine.state.objects[&new_object].card)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(discards.len(), 1);
            (discards[0], marker)
        };
        let result = run();
        assert_eq!(run(), result, "the same seed replays the same discard");
        outcomes.insert(result.0 == result.1);
    }
    assert_eq!(
        outcomes.len(),
        2,
        "random discard can keep or discard the tutored card"
    );
}

#[test]
fn sacrificed_fetch_land_puts_the_selected_land_onto_the_battlefield() {
    let mut t = Table::default();
    let fetch = t.card("", "Land", None,
        "{T}, Pay 1 life, Sacrifice this land: Search your library for a Mountain or Forest card, put it onto the battlefield, then shuffle.");
    let mountain = t.card("", "Basic Land — Mountain", None, "");
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    let source = g.put(fetch, P0, Zone::Battlefield);
    let pick = g.put(forest, P0, Zone::Library);
    g.put(mountain, P0, Zone::Library);
    g.main();
    g.act(activate(source, 0), &[], &[Answer::Objects(vec![pick])]);
    assert_eq!(g.life(P0), 19);
    assert!(g.find(fetch).is_none());
    let found = g
        .find(forest)
        .expect("selected forest reaches the battlefield");
    assert!(!g.engine.state.objects[&found].tapped);
    assert_eq!(g.engine.state.objects[&found].controller, P0);
}

#[test]
fn a_tutor_to_the_top_puts_the_card_on_top_after_shuffling() {
    let mut t = Table::default();
    let tutor = t.card(
        "{1}{B}",
        "Sorcery",
        None,
        "Search your library for a card, reveal it, then shuffle and put that card on top.",
    );
    let marker = t.card("{5}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(tutor, P0, Zone::Hand);
    g.main();
    let m = g.put(marker, P0, Zone::Library);
    g.act(Action::Cast { object: s }, &[], &[Answer::Objects(vec![m])]);
    let lib = ZoneRef::of(Zone::Library, P0);
    let top = g.engine.state.objects_in(lib)[0];
    assert_eq!(g.engine.state.objects[&top].card, marker);
}

#[test]
fn a_filtered_tutor_using_the_card_puts_the_selected_card_on_top() {
    let mut t = Table::default();
    let tutor = t.card(
        "{G}",
        "Instant",
        None,
        "Search your library for a creature card, reveal it, then shuffle and put the card on top.",
    );
    let marker = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(tutor, P0, Zone::Hand);
    g.main();
    let selected = g.put(marker, P0, Zone::Library);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![selected])],
    );
    let top = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[0];
    assert_eq!(g.engine.state.objects[&top].card, marker);
    assert_ne!(top, selected, "a selected card gets a new zone identity");
}

#[test]
fn an_unrestricted_two_card_tutor_moves_both_selected_cards_to_hand() {
    let mut t = Table::default();
    let tutor = t.card(
        "{B}",
        "Sorcery",
        None,
        "Search your library for two cards, put them into your hand, then shuffle your library.",
    );
    let first = t.card("{4}", "Artifact", None, "");
    let second = t.card("{3}", "Enchantment", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(tutor, P0, Zone::Hand);
    g.main();
    let first_pick = g.put(first, P0, Zone::Library);
    let second_pick = g.put(second, P0, Zone::Library);
    let hand_before = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![first_pick, second_pick])],
    );
    let hand = g.engine.state.objects_in(ZoneRef::of(Zone::Hand, P0));
    assert_eq!(hand.len(), hand_before + 1);
    for card in [first, second] {
        assert!(
            hand.iter()
                .any(|id| g.engine.state.objects[id].card == card)
        );
    }
}

#[test]
fn searching_for_cards_named_like_this_one() {
    let mut t = Table::default();
    let rat = t.card(
        "{1}{B}",
        "Creature — Rat",
        Some((1, 1)),
        "When this creature enters, you may search your library for any number of cards named \
         ~, reveal them, put them into your hand, then shuffle.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let r = g.put(rat, P0, Zone::Hand);
    g.main();
    let r2 = g.put(rat, P0, Zone::Library);
    let r3 = g.put(rat, P0, Zone::Library);
    let b = g.put(bear, P0, Zone::Library);
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: r },
        &[],
        &[Answer::Bool(true), Answer::Objects(vec![r2, r3])],
    );
    let _ = (hand, b);
    let in_hand: Vec<_> = g
        .engine
        .state
        .objects_in(ZoneRef::of(Zone::Hand, P0))
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert_eq!(in_hand.iter().filter(|c| **c == rat).count(), 2);
    assert!(!in_hand.contains(&bear));
}

#[test]
fn search_for_a_card_and_put_it_into_the_graveyard() {
    let mut t = Table::default();
    let entomb = t.card(
        "{B}",
        "Instant",
        None,
        "Search your library for a creature card, reveal that card, put it into your \
         graveyard, then shuffle.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(entomb, P0, Zone::Hand);
    g.main();
    let before = g.count(Zone::Library, P0);
    let pick = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[5];
    g.act(
        Action::Cast { object: s },
        &[],
        &[Answer::Objects(vec![pick])],
    );
    assert_eq!(g.count(Zone::Library, P0), before - 1);
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        2,
        "the found card and the instant"
    );
}

#[test]
fn revealed_tutors_reveal_the_selection_before_moving_or_shuffling() {
    for destination in [
        "put it into your hand, then shuffle",
        "then shuffle and put it on top",
    ] {
        let mut t = Table::default();
        let tutor = t.card(
            "{G}",
            "Sorcery",
            None,
            &format!("Search your library for a creature card, reveal it, {destination}."),
        );
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let selected = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[0];
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Objects(vec![selected])],
        );
        let revealed = g
            .engine
            .log
            .iter()
            .position(|entry| {
                matches!(entry.event,
            Event::Revealed { object } if object == selected)
            })
            .expect("selected card is revealed");
        let moved = g
            .engine
            .log
            .iter()
            .position(|entry| {
                matches!(entry.event,
            Event::ZoneChange { object, .. } if object == selected)
            })
            .unwrap();
        let shuffled = g
            .engine
            .log
            .iter()
            .position(|entry| matches!(entry.event, Event::Shuffled { player: P0 }))
            .unwrap();
        assert!(revealed < moved && revealed < shuffled);
        let opponent = g.engine.view_for(P1);
        assert_eq!(opponent.revealed_cards.len(), 1);
        assert_eq!(
            opponent.revealed_cards[0].card,
            g.engine.state.revealed_cards[0].card
        );
        assert!(
            !opponent
                .visible
                .values()
                .any(|card| card.zone.zone == Zone::Library),
            "announcing an identity must not reveal hidden library object ids or order"
        );
        if destination.starts_with("put") {
            assert!(
                opponent
                    .visible
                    .values()
                    .filter(|card| card.zone.zone == Zone::Hand)
                    .all(|card| card.card.is_none()),
                "opponent hand stays redacted"
            );
        }
        assert_eq!(
            g.engine
                .log
                .iter()
                .filter(|entry| matches!(entry.event,
            Event::Revealed { object } if object == selected))
                .count(),
            1
        );
    }
}

#[test]
fn mandatory_two_card_search_cannot_count_a_duplicate_answer_twice() {
    let mut t = Table::default();
    let tutor = t.card(
        "{1}",
        "Sorcery",
        None,
        "Search your library for two cards, put them into your hand, then shuffle.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(tutor, P0, Zone::Hand);
    g.main();
    let selected = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[0];
    let before = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![selected, selected])],
    );
    assert_eq!(
        g.count(Zone::Hand, P0),
        before + 1,
        "cast one, find two distinct cards"
    );
}

#[test]
fn tutor_destinations_and_tapped_status_match_printed_instructions() {
    for (instruction, zone, tapped) in [
        ("put it onto the battlefield", Zone::Battlefield, false),
        (
            "put it onto the battlefield tapped",
            Zone::Battlefield,
            true,
        ),
        ("put it into your graveyard", Zone::Graveyard, false),
        ("exile it", Zone::Exile, false),
        ("put it into your hand", Zone::Hand, false),
    ] {
        let mut t = Table::default();
        let tutor = t.card(
            "{1}",
            "Sorcery",
            None,
            &format!("Search your library for a creature card, {instruction}, then shuffle."),
        );
        let marker = t.card("{7}", "Creature", Some((7, 7)), "");
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let selected = g.put(marker, P0, Zone::Library);
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Objects(vec![selected])],
        );
        let found = g
            .engine
            .state
            .objects
            .values()
            .find(|object| object.card == marker)
            .unwrap();
        assert_eq!(found.zone.zone, zone);
        assert_eq!(found.owner, P0);
        assert_eq!(found.controller, P0);
        assert_eq!(found.tapped, tapped);
        assert!(
            g.engine
                .log
                .iter()
                .any(|entry| matches!(entry.event, Event::Shuffled { player: P0 }))
        );
    }
}

#[test]
fn a_two_card_tutor_handles_empty_and_short_libraries() {
    for available in [0, 1] {
        let mut t = Table::default();
        let tutor = t.card(
            "{1}",
            "Sorcery",
            None,
            "Search your library for two cards, put them into your hand, then shuffle.",
        );
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let library = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
        for id in library.iter().skip(available) {
            g.engine.state.objects.remove(id);
        }
        g.engine.state.zone_order.insert(
            ZoneRef::of(Zone::Library, P0),
            library.into_iter().take(available).collect(),
        );
        let hand = g.count(Zone::Hand, P0);
        g.act(Action::Cast { object: spell }, &[], &[]);
        assert_eq!(g.count(Zone::Library, P0), 0);
        assert_eq!(g.count(Zone::Hand, P0), hand - 1 + available);
        assert!(
            g.engine
                .log
                .iter()
                .any(|entry| matches!(entry.event, Event::Shuffled { player: P0 }))
        );
    }
}

#[test]
fn a_filtered_search_with_no_matches_still_shuffles() {
    let mut t = Table::default();
    let tutor = t.card(
        "{1}",
        "Sorcery",
        None,
        "Search your library for an artifact card, reveal it, put it into your hand, then shuffle.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(tutor, P0, Zone::Hand);
    g.main();
    let library = g.count(Zone::Library, P0);
    let hand = g.count(Zone::Hand, P0);
    g.act(Action::Cast { object: spell }, &[], &[]);
    assert_eq!(g.count(Zone::Library, P0), library);
    assert_eq!(g.count(Zone::Hand, P0), hand - 1);
    assert!(g.engine.state.revealed_cards.is_empty());
    assert!(
        g.engine
            .log
            .iter()
            .any(|entry| matches!(entry.event, Event::Shuffled { player: P0 }))
    );
}
