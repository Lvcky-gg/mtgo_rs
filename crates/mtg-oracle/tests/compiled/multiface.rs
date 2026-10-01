//! Invented modal double-faced cards through compiler, engine, view and bot.
use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
    view::project,
};
use mtg_ir::Layout;

fn spell_land() -> (Table, mtg_core::CardId) {
    let mut t = Table::default();
    let front = t.card("{3}{R}", "Sorcery", None, "~ deals 4 damage to any target.");
    let back = t.card("", "Land", None, "~ enters tapped.\n{T}: Add {U}.");
    t.pair(front, back, Layout::ModalDfc);
    (t, front)
}

#[test]
fn land_face_enters_tapped_uses_its_mana_and_consumes_the_land_play() {
    let (t, card) = spell_land();
    let mut g = Game::new(t);
    let object = g.put(card, P0, Zone::Hand);
    let another = g.put(card, P0, Zone::Hand);
    let actions = g.main();
    assert!(actions.contains(&Action::PlayLandFace { object, face: 1 }));
    assert!(!actions.contains(&Action::Cast { object }));
    assert_eq!(g.engine.state.objects[&object].face, 0);
    g.act(Action::PlayLandFace { object, face: 1 }, &[], &[]);
    let land = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == card && o.zone.zone == Zone::Battlefield)
        .unwrap();
    assert_eq!(land.face, 1);
    assert!(land.tapped);
    let id = land.id;
    assert_eq!(
        g.engine.characteristics(&g.table, id).unwrap().card_types,
        vec![mtg_core::CardType::Land]
    );
    assert!(!g.main().contains(&Action::PlayLandFace {
        object: another,
        face: 1
    }));
    g.until(P0, Step::PostcombatMain);
    // Still tapped this turn; the next untap makes only the back's blue ability available.
    g.until(P0, Step::PrecombatMain);
    let Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
        panic!()
    };
    let ChoiceKind::Priority { legal } = c.kind else {
        panic!()
    };
    assert!(legal.mana_abilities.iter().any(
        |a| matches!(a, Action::ActivateManaAbility { source, color: None, .. } if *source == id)
    ));
}

#[test]
fn back_spell_uses_its_own_cost_targets_effect_and_public_face() {
    let mut t = Table::default();
    let front = t.card("{6}{G}", "Creature", Some((7, 7)), "");
    let back = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 3 damage to target creature.",
    );
    t.pair(front, back, Layout::ModalDfc);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let victim = g.put(bear, P1, Zone::Battlefield);
    let object = g.put(front, P0, Zone::Hand);
    let actions = g.main();
    assert!(!actions.contains(&Action::Cast { object }));
    assert!(actions.contains(&Action::CastFace { object, face: 1 }));
    g.act_holding(
        Action::CastFace { object, face: 1 },
        &[Target::Object(victim)],
    );
    let spell = g.stack()[0];
    let view = project(&g.engine.state, P1);
    assert_eq!(view.visible[&spell].face, 1);
    assert_eq!(
        g.engine
            .characteristics(&g.table, spell)
            .unwrap()
            .mana_cost
            .mana_value(),
        1
    );
    g.main();
    assert!(!g.engine.state.objects.contains_key(&victim));
    let grave = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == front && o.zone.zone == Zone::Graveyard)
        .unwrap()
        .id;
    assert_eq!(g.engine.state.objects[&grave].zone.zone, Zone::Graveyard);
    assert_eq!(g.engine.state.objects[&grave].face, 0);
}

#[test]
fn each_face_has_independent_timing_and_target_legality() {
    let mut t = Table::default();
    let front = t.card("{R}", "Sorcery", None, "You gain 2 life.");
    let back = t.card("{R}", "Instant", None, "Destroy target creature.");
    t.pair(front, back, Layout::ModalDfc);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let object = g.put(front, P0, Zone::Hand);
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object }));
    assert!(!actions.contains(&Action::CastFace { object, face: 1 }));
    g.put(bear, P1, Zone::Battlefield);
    let actions = g.until(P0, Step::BeginCombat);
    assert!(!actions.contains(&Action::Cast { object }));
    assert!(actions.contains(&Action::CastFace { object, face: 1 }));
}

#[test]
fn a_land_front_does_not_stop_the_back_spell_from_being_cast() {
    let mut t = Table::default();
    let front = t.card("", "Land", None, "{T}: Add {R}.");
    let back = t.card("{R}", "Sorcery", None, "You gain 5 life.");
    t.pair(front, back, Layout::ModalDfc);
    let mut g = Game::new(t);
    g.lands(1);
    let object = g.put(front, P0, Zone::Hand);
    let actions = g.main();
    assert!(actions.contains(&Action::PlayLand { object }));
    assert!(actions.contains(&Action::CastFace { object, face: 1 }));
    g.act(Action::CastFace { object, face: 1 }, &[], &[]);
    assert_eq!(g.life(P0), 25);
    assert_eq!(g.engine.state.player(P0).lands_played, 0);
}

#[test]
fn back_permanent_retains_face_and_fires_its_enter_and_die_triggers() {
    let mut t = Table::default();
    let front = t.card("{8}", "Artifact", None, "");
    let back = t.card(
        "{R}",
        "Creature",
        Some((2, 1)),
        "When ~ enters, you gain 3 life.\nWhen ~ dies, you gain 4 life.",
    );
    t.pair(front, back, Layout::ModalDfc);
    let burn = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 1 damage to target creature.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let object = g.put(front, P0, Zone::Hand);
    let spell = g.put(burn, P0, Zone::Hand);
    g.main();
    g.act(Action::CastFace { object, face: 1 }, &[], &[]);
    let creature = g.find(front).unwrap();
    assert_eq!(g.engine.state.objects[&creature].face, 1);
    assert_eq!(g.life(P0), 23);
    g.act(
        Action::Cast { object: spell },
        &[Target::Object(creature)],
        &[],
    );
    assert_eq!(g.life(P0), 27);
    let grave = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == front && o.zone.zone == Zone::Graveyard)
        .unwrap()
        .id;
    assert_eq!(g.engine.state.objects[&grave].face, 0);
}

#[test]
fn only_modal_cards_offer_face_casting_and_invalid_faces_do_not_mutate_state() {
    let (mut t, card) = spell_land();
    // Same two faces, but a transforming layout never grants choice of the back.
    let front = t.card("{R}", "Creature", Some((1, 1)), "");
    let back = t.card("{R}", "Creature", Some((2, 2)), "");
    t.pair(front, back, Layout::Transforming);
    let mut g = Game::new(t);
    g.lands(1);
    let object = g.put(card, P0, Zone::Hand);
    let transforming = g.put(front, P0, Zone::Hand);
    let actions = g.main();
    assert!(!actions.contains(&Action::CastFace {
        object: transforming,
        face: 1
    }));
    let Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
        panic!()
    };
    let log_len = g.engine.log.len();
    for action in [
        Action::CastFace { object, face: 7 },
        Action::PlayLandFace { object, face: 0 },
        Action::CastFace {
            object: transforming,
            face: 1,
        },
    ] {
        assert!(
            g.engine
                .answer(&g.table, c.id, Answer::Action(action))
                .is_err()
        );
        assert_eq!(g.engine.state.objects[&object].face, 0);
        assert_eq!(g.engine.state.player(P0).lands_played, 0);
        assert_eq!(g.engine.log.len(), log_len);
    }
}

#[test]
fn playing_a_land_face_can_be_undone_without_revealing_new_information() {
    let (t, card) = spell_land();
    let mut g = Game::new(t);
    let object = g.put(card, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::PlayLandFace { object, face: 1 }, &[]);
    assert!(g.engine.can_undo(P0));
    let Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
        panic!()
    };
    g.engine.answer(&g.table, c.id, Answer::Undo).unwrap();
    assert_eq!(g.engine.state.objects[&object].face, 0);
    assert_eq!(g.engine.state.objects[&object].zone.zone, Zone::Hand);
    assert_eq!(g.engine.state.player(P0).lands_played, 0);
}

#[test]
fn the_bot_chooses_a_land_face_and_knows_its_battlefield_stats() {
    let (t, card) = spell_land();
    let mut g = Game::new(t);
    let object = g.put(card, P0, Zone::Hand);
    g.main();
    let bot = mtg_policy::bot::Bot::new(&g.table, [card]);
    let Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
        panic!()
    };
    let answer = bot.decide(&c, &project(&g.engine.state, P0));
    assert!(matches!(
        answer,
        Answer::Action(Action::PlayLandFace { face: 1, .. })
    ));
    g.engine.answer(&g.table, c.id, answer).unwrap();
    for _ in 0..100 {
        if let Progress::NeedsChoice(_) = g.engine.advance(&g.table) {
            break;
        }
    }
    assert!(!g.engine.state.objects.contains_key(&object));
    assert_eq!(g.engine.state.player(P0).lands_played, 1);
    let land = g.find(card).unwrap();
    assert_eq!(project(&g.engine.state, P0).visible[&land].face, 1);
}

#[test]
fn flashback_permission_belongs_to_the_chosen_face_and_never_plays_a_land() {
    let mut t = Table::default();
    let front = t.card("{R}", "Sorcery", None, "You gain 2 life.\nFlashback {R}");
    let back = t.card("{R}", "Instant", None, "You gain 3 life.");
    t.pair(front, back, Layout::ModalDfc);
    let mut g = Game::new(t);
    g.lands(1);
    let object = g.put(front, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object }));
    assert!(!actions.contains(&Action::CastFace { object, face: 1 }));

    let (t, card) = spell_land();
    let mut g = Game::new(t);
    let object = g.put(card, P0, Zone::Graveyard);
    assert!(!g.main().contains(&Action::PlayLandFace { object, face: 1 }));
}

#[test]
fn commander_land_back_is_not_playable_from_the_command_zone() {
    let (t, card) = spell_land();
    let mut g = Game::new(t);
    g.engine.state.commander.commanders.insert(P0, card);
    let object = g.put(card, P0, Zone::Command);
    assert!(!g.main().contains(&Action::PlayLandFace { object, face: 1 }));
}

#[test]
fn back_face_actions_round_trip_on_the_wire() {
    for action in [
        Action::CastFace {
            object: mtg_core::ObjectId(3),
            face: 1,
        },
        Action::PlayLandFace {
            object: mtg_core::ObjectId(7),
            face: 1,
        },
    ] {
        let encoded = serde_json::to_string(&Answer::Action(action.clone())).unwrap();
        let decoded: Answer = serde_json::from_str(&encoded).unwrap();
        assert!(matches!(decoded, Answer::Action(a) if a == action));
    }
}

#[test]
fn bouncing_a_back_permanent_returns_the_front_to_hand() {
    let mut t = Table::default();
    let front = t.card("{5}{G}", "Creature", Some((5, 5)), "");
    let back = t.card("{R}", "Creature", Some((1, 2)), "");
    t.pair(front, back, Layout::ModalDfc);
    let bounce = t.card(
        "{U}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let object = g.put(front, P0, Zone::Hand);
    let spell = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.act(Action::CastFace { object, face: 1 }, &[], &[]);
    let creature = g.find(front).unwrap();
    g.act(
        Action::Cast { object: spell },
        &[Target::Object(creature)],
        &[],
    );
    let hand = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == front && o.zone.zone == Zone::Hand)
        .unwrap();
    assert_eq!(hand.face, 0);
    let id = hand.id;
    assert_eq!(
        g.engine.characteristics(&g.table, id).unwrap().power,
        Some(5)
    );
}

#[test]
fn back_face_additional_life_cost_is_paid_and_checked() {
    let mut t = Table::default();
    let front = t.card("{8}", "Artifact", None, "");
    let back = t.card(
        "{R}",
        "Sorcery",
        None,
        "As an additional cost to cast this spell, pay 3 life.\nYou gain 1 life.",
    );
    t.pair(front, back, Layout::ModalDfc);
    let mut g = Game::new(t);
    g.lands(2);
    let object = g.put(front, P0, Zone::Hand);
    g.main();
    g.act(Action::CastFace { object, face: 1 }, &[], &[]);
    assert_eq!(g.life(P0), 18);
    let mut g = Game::new(g.table);
    g.lands(1);
    let another = g.put(front, P0, Zone::Hand);
    g.engine.state.players.get_mut(&P0).unwrap().life = 2;
    assert!(!g.main().contains(&Action::CastFace {
        object: another,
        face: 1
    }));
}

#[test]
fn bot_casts_the_expensive_back_and_targets_its_effect() {
    let mut t = Table::default();
    let front = t.card("{R}", "Sorcery", None, "Target player gains 2 life.");
    let back = t.card(
        "{1}{R}",
        "Sorcery",
        None,
        "~ deals 3 damage to target player.",
    );
    t.pair(front, back, Layout::ModalDfc);
    let mut g = Game::new(t);
    g.lands(2);
    let object = g.put(front, P0, Zone::Hand);
    g.main();
    let bot = mtg_policy::bot::Bot::new(&g.table, [front]);
    let Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
        panic!()
    };
    let answer = bot.decide(&c, &project(&g.engine.state, P0));
    assert!(matches!(
        answer,
        Answer::Action(Action::CastFace { face: 1, .. })
    ));
    g.engine.answer(&g.table, c.id, answer).unwrap();
    let mut targeted = false;
    for _ in 0..100 {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => {
                if matches!(c.kind, ChoiceKind::Priority { .. }) && g.stack().is_empty() {
                    break;
                }
                let answer = bot.decide(&c, &project(&g.engine.state, c.who));
                if let Answer::Targets(picks) = &answer {
                    assert_eq!(picks, &vec![vec![Target::Player(P1)]]);
                    targeted = true;
                }
                g.engine.answer(&g.table, c.id, answer).unwrap();
            }
            _ => panic!(),
        }
    }
    assert!(targeted);
    assert_eq!(g.life(P1), 17);
    assert!(!g.engine.state.objects.contains_key(&object));
}

#[test]
fn returning_a_spell_front_mdfc_to_the_battlefield_keeps_it_in_the_graveyard() {
    let mut t = Table::default();
    let front = t.card("{R}", "Instant", None, "You gain 1 life.");
    let back = t.card("{R}", "Creature", Some((1, 1)), "");
    t.pair(front, back, Layout::ModalDfc);
    let returner = t.card(
        "{W}",
        "Sorcery",
        None,
        "Return target card from your graveyard to the battlefield.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let object = g.put(front, P0, Zone::Graveyard);
    let spell = g.put(returner, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[Target::Object(object)],
        &[],
    );
    assert_eq!(g.engine.state.objects[&object].zone.zone, Zone::Graveyard);
    assert_eq!(g.engine.state.objects[&object].face, 0);
}

#[test]
fn restricted_mana_pays_only_for_what_it_names() {
    let mut t = Table::default();
    let shop = t.card(
        "",
        "Land",
        None,
        "{T}: Add {C}{C}. Spend this mana only to cast artifact spells.",
    );
    let rock = t.card("{2}", "Artifact", None, "");
    let bear = t.card("{2}", "Creature — Bear", Some((2, 2)), "");
    let mut g = Game::new(t);
    let s = g.put(shop, P0, Zone::Battlefield);
    let r = g.put(rock, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Hand);
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object: r }), "an artifact");
    assert!(
        !actions.contains(&Action::Cast { object: b }),
        "not a creature"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::ActivateManaAbility { source, .. } if *source == s)),
        "restricted mana can't be floated"
    );
    g.cast(r, &[]);
    assert!(g.find(rock).is_some());
}
