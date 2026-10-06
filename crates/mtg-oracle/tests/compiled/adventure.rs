//! Invented Adventure cards, cast through the compiler and engine.
use super::harness::*;
use mtg_core::{Cause, Event, Step, Target, Zone, ZoneRef};
use mtg_engine::{Progress, actions::Action, choice::Answer, view::project};
use mtg_ir::Layout;

#[test]
fn claim_territory_splits_forests_then_shuffles_the_omen_card_into_its_library() {
    let mut t = Table::default();
    let front = t.card("{4}{G}{G}", "Creature — Dragon", Some((4, 5)), "Flying");
    let omen = t.card("{2}{G}", "Sorcery — Omen", None,
        "Search your library for up to two basic Forest cards, reveal them, put one onto the battlefield tapped and the other into your hand, then shuffle. (Also shuffle this card.)");
    t.pair(front, omen, Layout::Adventure);
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    let spell = g.put(front, P0, Zone::Hand);
    g.main();
    let first = g.put(forest, P0, Zone::Library);
    let second = g.put(forest, P0, Zone::Library);
    g.act(
        Action::CastFace {
            object: spell,
            face: 1,
        },
        &[],
        &[
            Answer::Objects(vec![first, second]),
            Answer::Objects(vec![second]),
        ],
    );
    let card = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == front)
        .unwrap();
    assert_eq!(card.zone, ZoneRef::of(Zone::Library, P0));
    assert_eq!(card.face, 0);
    assert_eq!(card.adventure_player, None);
    assert_eq!(
        g.engine
            .log
            .iter()
            .filter(|entry| matches!(entry.event, Event::Shuffled { player: P0 }))
            .count(),
        2
    );
    assert_eq!(
        g.engine
            .state
            .objects
            .values()
            .filter(|o| o.card == forest && o.zone.zone == Zone::Battlefield && o.tapped)
            .count(),
        1
    );
    assert_eq!(
        g.engine
            .state
            .objects
            .values()
            .filter(|o| o.card == forest && o.zone.zone == Zone::Hand)
            .count(),
        1
    );
}

#[test]
fn countered_or_fizzled_omens_go_to_the_graveyard_without_shuffling() {
    for countered in [false, true] {
        let mut t = Table::default();
        let front = t.card("{4}{R}", "Creature — Dragon", Some((4, 4)), "Flying");
        let omen = t.card(
            "{R}",
            "Instant — Omen",
            None,
            "~ deals 2 damage to target creature. (Also shuffle this card.)",
        );
        t.pair(front, omen, Layout::Adventure);
        let bear = t.bear();
        let response = t.card(
            "{U}",
            "Instant",
            None,
            if countered {
                "Counter target spell."
            } else {
                "Destroy target creature."
            },
        );
        let mut g = Game::new(t);
        g.lands(2);
        let target = g.put(bear, P1, Zone::Battlefield);
        let spell = g.put(front, P0, Zone::Hand);
        let answer = g.put(response, P0, Zone::Hand);
        g.main();
        g.act_holding(
            Action::CastFace {
                object: spell,
                face: 1,
            },
            &[Target::Object(target)],
        );
        let response_target = if countered { g.stack()[0] } else { target };
        g.act(
            Action::Cast { object: answer },
            &[Target::Object(response_target)],
            &[],
        );
        let card = g
            .engine
            .state
            .objects
            .values()
            .find(|o| o.card == front)
            .unwrap();
        assert_eq!(card.zone.zone, Zone::Graveyard);
        assert_eq!(card.adventure_player, None);
        assert!(
            !g.engine
                .log
                .iter()
                .any(|entry| matches!(entry.event, Event::Shuffled { .. }))
        );
    }
}

#[test]
fn casting_the_creature_half_of_an_omen_enters_the_battlefield() {
    let mut t = Table::default();
    let front = t.card("{1}{R}", "Creature — Dragon", Some((2, 2)), "Flying");
    let omen = t.card(
        "{R}",
        "Sorcery — Omen",
        None,
        "You gain 2 life. (Also shuffle this card.)",
    );
    t.pair(front, omen, Layout::Adventure);
    let mut g = Game::new(t);
    g.lands(2);
    let spell = g.put(front, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    assert!(g.find(front).is_some());
    assert_eq!(g.life(P0), 20);
    assert!(
        !g.engine
            .log
            .iter()
            .any(|entry| matches!(entry.event, Event::Shuffled { .. }))
    );
}

#[test]
fn copied_omens_resolve_and_shuffle_without_leaving_a_fake_card_in_the_library() {
    let mut t = Table::default();
    let front = t.card("{4}{R}", "Creature — Dragon", Some((4, 4)), "Flying");
    let omen = t.card(
        "{R}",
        "Sorcery — Omen",
        None,
        "You gain 2 life. (Also shuffle this card.)",
    );
    t.pair(front, omen, Layout::Adventure);
    let copier = t.card(
        "{U}",
        "Instant",
        None,
        "Copy target instant or sorcery spell. You may choose new targets for the copy.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let source = g.put(front, P0, Zone::Hand);
    let response = g.put(copier, P0, Zone::Hand);
    g.main();
    g.act_holding(
        Action::CastFace {
            object: source,
            face: 1,
        },
        &[],
    );
    let spell = g.stack()[0];
    g.act(
        Action::Cast { object: response },
        &[Target::Object(spell)],
        &[],
    );
    assert_eq!(g.life(P0), 24);
    let cards: Vec<_> = g
        .engine
        .state
        .objects
        .values()
        .filter(|o| o.card == front)
        .collect();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].zone, ZoneRef::of(Zone::Library, P0));
    assert_eq!(
        g.engine
            .log
            .iter()
            .filter(|entry| matches!(entry.event, Event::Shuffled { player: P0 }))
            .count(),
        2
    );
}

fn cards() -> (Table, mtg_core::CardId) {
    let mut t = Table::default();
    let front = t.card("{2}{G}", "Creature", Some((3, 3)), "Reach");
    let adventure = t.card("{R}", "Instant — Adventure", None,
        "~ deals 2 damage to any target. (Then exile this card. You may cast the creature later from exile.)");
    t.pair(front, adventure, Layout::Adventure);
    (t, front)
}

fn exiled(g: &Game, card: mtg_core::CardId) -> mtg_core::ObjectId {
    g.engine
        .state
        .objects
        .values()
        .find(|o| o.card == card && o.zone.zone == Zone::Exile)
        .unwrap()
        .id
}

#[test]
fn resolving_the_adventure_exiles_the_front_and_then_allows_the_creature() {
    let (t, card) = cards();
    let mut g = Game::new(t);
    g.lands(4);
    let object = g.put(card, P0, Zone::Hand);
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object }));
    assert!(actions.contains(&Action::CastFace { object, face: 1 }));
    g.act(
        Action::CastFace { object, face: 1 },
        &[Target::Player(P1)],
        &[],
    );
    assert_eq!(g.life(P1), 18);
    let object = exiled(&g, card);
    assert_eq!(g.engine.state.objects[&object].face, 0);
    assert_eq!(g.engine.state.objects[&object].adventure_player, Some(P0));
    let view = project(&g.engine.state, P1);
    assert_eq!(view.visible[&object].adventure_player, Some(P0));
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object }));
    assert!(!actions.contains(&Action::CastFace { object, face: 1 }));
    g.act(Action::Cast { object }, &[], &[]);
    let creature = g.find(card).unwrap();
    assert_eq!(g.engine.state.objects[&creature].face, 0);
    assert_eq!(g.engine.state.objects[&creature].adventure_player, None);
    assert_eq!(g.pt(creature), (3, 3));
}

#[test]
fn an_adventure_with_all_targets_illegal_goes_to_graveyard_without_permission() {
    let (mut t, card) = cards();
    let bear = t.bear();
    let kill = t.card("{R}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    g.lands(2);
    let object = g.put(card, P0, Zone::Hand);
    let response = g.put(kill, P0, Zone::Hand);
    let target = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.act_holding(
        Action::CastFace { object, face: 1 },
        &[Target::Object(target)],
    );
    g.act(
        Action::Cast { object: response },
        &[Target::Object(target)],
        &[],
    );
    let grave = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == card)
        .unwrap();
    assert_eq!(grave.zone.zone, Zone::Graveyard);
    assert_eq!(grave.face, 0);
    assert_eq!(grave.adventure_player, None);
}

#[test]
fn a_countered_adventure_never_grants_an_exile_cast() {
    let (mut t, card) = cards();
    let counter = t.card("{U}", "Instant", None, "Counter target spell.");
    let mut g = Game::new(t);
    g.lands(2);
    let object = g.put(card, P0, Zone::Hand);
    let response = g.put(counter, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::CastFace { object, face: 1 }, &[Target::Player(P1)]);
    let top = g.stack()[0];
    g.act(
        Action::Cast { object: response },
        &[Target::Object(top)],
        &[],
    );
    assert_eq!(g.life(P1), 20);
    let grave = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == card)
        .unwrap();
    assert_eq!(grave.zone.zone, Zone::Graveyard);
    assert_eq!(grave.adventure_player, None);
}

#[test]
fn ordinary_exile_does_not_grant_permission_and_exile_cast_keeps_normal_timing() {
    let (t, card) = cards();
    let mut g = Game::new(t);
    g.lands(4);
    let arbitrary = g.put(card, P0, Zone::Exile);
    let hand = g.put(card, P0, Zone::Hand);
    assert!(!g.main().contains(&Action::Cast { object: arbitrary }));
    g.act(
        Action::CastFace {
            object: hand,
            face: 1,
        },
        &[Target::Player(P1)],
        &[],
    );
    let granted = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.adventure_player == Some(P0))
        .unwrap()
        .id;
    let actions = g.until(P0, Step::BeginCombat);
    assert!(
        !actions.contains(&Action::Cast { object: granted }),
        "the normal creature is not an instant"
    );
    assert!(!actions.contains(&Action::CastFace {
        object: granted,
        face: 1
    }));
}

#[test]
fn leaving_and_reentering_exile_loses_the_adventure_permission() {
    let (t, card) = cards();
    let mut g = Game::new(t);
    g.lands(1);
    let object = g.put(card, P0, Zone::Hand);
    g.main();
    g.act(
        Action::CastFace { object, face: 1 },
        &[Target::Player(P1)],
        &[],
    );
    let old = exiled(&g, card);
    let grave = g.engine.state.new_object_id();
    mtg_engine::apply::apply(
        &mut g.engine.state,
        Cause::TurnStructure,
        Event::ZoneChange {
            object: old,
            new_object: grave,
            from: ZoneRef::shared(Zone::Exile),
            to: ZoneRef::of(Zone::Graveyard, P0),
            index: None,
        },
        &mut g.engine.log,
    );
    assert_eq!(g.engine.state.objects[&grave].adventure_player, None);
    let new = g.engine.state.new_object_id();
    mtg_engine::apply::apply(
        &mut g.engine.state,
        Cause::TurnStructure,
        Event::ZoneChange {
            object: grave,
            new_object: new,
            from: ZoneRef::of(Zone::Graveyard, P0),
            to: ZoneRef::shared(Zone::Exile),
            index: None,
        },
        &mut g.engine.log,
    );
    assert_eq!(g.engine.state.objects[&new].adventure_player, None);
    let encoded = serde_json::to_value(project(&g.engine.state, P1)).unwrap();
    let decoded: mtg_engine::PlayerView = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.visible[&new].adventure_player, None);
}

#[test]
fn the_bot_uses_the_adventure_then_casts_the_creature_from_exile() {
    let (t, card) = cards();
    let mut g = Game::new(t);
    g.lands(4);
    let object = g.put(card, P0, Zone::Hand);
    g.main();
    let bot = mtg_policy::bot::Bot::new(&g.table, [card]);
    let mut choices = Vec::new();
    for _ in 0..1000 {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => {
                if g.find(card).is_some() && g.stack().is_empty() {
                    break;
                }
                let view = project(&g.engine.state, c.who);
                let answer = if c.who == P0 {
                    bot.decide(&c, &view)
                } else {
                    c.default.clone().unwrap_or(Answer::Pass)
                };
                if let Answer::Action(a) = &answer {
                    choices.push(a.clone());
                }
                g.engine.answer(&g.table, c.id, answer).unwrap();
            }
            _ => panic!(),
        }
    }
    assert_eq!(choices.first(), Some(&Action::CastFace { object, face: 1 }));
    assert!(choices.iter().any(|a| matches!(a, Action::Cast { .. })));
    assert!(g.find(card).is_some());
    assert_eq!(g.life(P1), 18);
}

#[test]
fn the_permission_belongs_to_the_resolving_controller_and_survives_until_used() {
    let (t, card) = cards();
    let mut g = Game::new(t);
    g.lands(1);
    let object = g.put(card, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::CastFace { object, face: 1 }, &[Target::Player(P1)]);
    let top = g.stack()[0];
    // Simulate a supported game's spell-controller change before resolution.
    g.engine.state.objects.get_mut(&top).unwrap().controller = P1;
    g.main();
    let object = exiled(&g, card);
    assert_eq!(g.engine.state.objects[&object].owner, P0);
    assert_eq!(g.engine.state.objects[&object].adventure_player, Some(P1));
    assert!(!g.main().contains(&Action::Cast { object }));
    let land = g
        .table
        .card("", "Land", None, "{T}: Add one mana of any color.");
    for _ in 0..3 {
        g.put(land, P1, Zone::Battlefield);
    }
    let actions = g.until(P1, Step::PrecombatMain);
    assert!(actions.contains(&Action::Cast { object }));
    assert!(!actions.contains(&Action::CastFace { object, face: 1 }));
}

#[test]
fn casting_the_normal_card_does_not_send_it_on_an_adventure() {
    let (t, card) = cards();
    let mut g = Game::new(t);
    g.lands(3);
    let object = g.put(card, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object }, &[], &[]);
    let creature = g.find(card).unwrap();
    assert_eq!(g.engine.state.objects[&creature].adventure_player, None);
    assert_eq!(g.engine.state.objects[&creature].face, 0);
    assert_eq!(g.life(P1), 20);
}
