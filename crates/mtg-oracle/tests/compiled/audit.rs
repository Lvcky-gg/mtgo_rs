//! Rules-audit regressions through the same compiler used for imported cards.
use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn instants_are_responses_but_sorceries_need_own_main_and_empty_stack() {
    let mut table = Table::default();
    let instant = table.card("", "Instant", None, "You gain 1 life.");
    let sorcery = table.card("", "Sorcery", None, "You gain 2 life.");
    let mut game = Game::new(table);
    let first = game.put(instant, P0, Zone::Hand);
    let response = game.put(instant, P0, Zone::Hand);
    let slow = game.put(sorcery, P0, Zone::Hand);
    let actions = game.main();
    assert!(actions.contains(&Action::Cast { object: slow }));
    game.act_holding(Action::Cast { object: first }, &[]);
    let actions = match &game.pending.as_ref().unwrap().kind {
        mtg_engine::choice::ChoiceKind::Priority { legal } => &legal.actions,
        _ => panic!("priority expected"),
    };
    assert!(actions.contains(&Action::Cast { object: response }));
    assert!(!actions.contains(&Action::Cast { object: slow }));
    game.act(Action::Cast { object: response }, &[], &[]);
    assert_eq!(game.life(P0), 22, "both instants resolved");
    let later = game.put(instant, P0, Zone::Hand);
    let actions = game.until(P0, Step::BeginCombat);
    assert!(actions.contains(&Action::Cast { object: later }));
    assert!(!actions.contains(&Action::Cast { object: slow }));
    let actions = game.until(P1, Step::PrecombatMain);
    assert!(!actions.contains(&Action::Cast { object: slow }));
    let priority = game.pending.take().unwrap();
    game.engine
        .answer(&game.table, priority.id, Answer::Pass)
        .unwrap();
    loop {
        if let mtg_engine::Progress::NeedsChoice(question) = game.engine.advance(&game.table) {
            assert_eq!(question.who, P0);
            assert_eq!(game.engine.state.active_player, P1);
            let mtg_engine::choice::ChoiceKind::Priority { legal } = question.kind else {
                panic!("priority expected")
            };
            assert!(legal.actions.contains(&Action::Cast { object: later }));
            assert!(
                !legal.actions.contains(&Action::Cast { object: slow }),
                "opponent's turn"
            );
            break;
        }
    }
}

#[test]
fn creatures_and_artifacts_enter_tapped_and_cannot_supply_tap_mana() {
    let mut table = Table::default();
    let creature = table.card("", "Creature", Some((2, 2)), "This creature enters tapped.");
    let artifact = table.card(
        "",
        "Artifact",
        None,
        "This artifact enters tapped.\n{T}: Add {G}.",
    );
    let mut game = Game::new(table);
    let creature_spell = game.put(creature, P0, Zone::Hand);
    let artifact_spell = game.put(artifact, P0, Zone::Hand);
    game.main();
    game.cast(creature_spell, &[]);
    game.cast(artifact_spell, &[]);
    let creature = game.find(creature).unwrap();
    let artifact = game.find(artifact).unwrap();
    assert!(game.engine.state.objects[&creature].tapped);
    assert!(game.engine.state.objects[&artifact].tapped);
    assert!(game.pending.as_ref().is_some_and(|q| matches!(&q.kind,
        mtg_engine::choice::ChoiceKind::Priority { legal } if legal.mana_abilities.is_empty())));
}

#[test]
fn cleanup_triggers_allow_instants_and_repeat_cleanup_before_the_next_turn() {
    use mtg_engine::{Progress, choice::ChoiceKind};
    let mut table = Table::default();
    let watcher = table.card(
        "",
        "Enchantment",
        None,
        "Whenever you discard a card, you gain 1 life.",
    );
    let instant = table.card("", "Instant", None, "Draw two cards.");
    let filler = table.bear();
    let mut game = Game::new(table);
    game.put(watcher, P0, Zone::Battlefield);
    game.main();
    let spell = game.put(instant, P0, Zone::Hand);
    for _ in 0..6 {
        game.put(filler, P0, Zone::Hand);
    }
    assert_eq!(game.count(Zone::Hand, P0), 8);
    let priority = game.pending.take().unwrap();
    game.engine
        .answer(&game.table, priority.id, Answer::Pass)
        .unwrap();
    let mut cast = false;
    let mut discards = 0;
    for _ in 0..6000 {
        if game.engine.state.active_player == P1 {
            assert!(cast, "cleanup must grant priority for its discard trigger");
            assert_eq!(
                discards, 2,
                "drawing during cleanup requires another discard pass"
            );
            assert_eq!(game.count(Zone::Hand, P0), 7);
            assert_eq!(game.life(P0), 22);
            assert!(
                game.stack().is_empty(),
                "cleanup triggers resolve before the turn ends"
            );
            return;
        }
        let Progress::NeedsChoice(question) = game.engine.advance(&game.table) else {
            continue;
        };
        let answer = match &question.kind {
            ChoiceKind::ChooseObjects { from, min, .. }
                if game.engine.state.step == Step::Cleanup =>
            {
                discards += 1;
                Answer::Objects(
                    from.iter()
                        .copied()
                        .filter(|id| *id != spell)
                        .take(*min as usize)
                        .collect(),
                )
            }
            ChoiceKind::Priority { legal }
                if game.engine.state.step == Step::Cleanup && question.who == P0 && !cast =>
            {
                assert!(legal.actions.contains(&Action::Cast { object: spell }));
                cast = true;
                Answer::Action(Action::Cast { object: spell })
            }
            _ => question.default.clone().unwrap_or(Answer::Pass),
        };
        game.engine
            .answer(&game.table, question.id, answer)
            .unwrap();
    }
    panic!("cleanup must finish after resolving its triggers");
}

#[test]
fn paying_or_declining_shockland_life_controls_its_entry_status() {
    for (pay, life, tapped, after) in [
        (false, 20, true, 20),
        (true, 20, false, 18),
        (true, 1, true, 1),
    ] {
        let mut table = Table::default();
        let land = table.card(
            "",
            "Land — Island",
            None,
            "As this land enters, you may pay 2 life. If you don't, it enters tapped.",
        );
        let mut game = Game::new(table);
        let card = game.put(land, P0, Zone::Hand);
        game.engine.state.players.get_mut(&P0).unwrap().life = life;
        game.main();
        game.act(Action::PlayLand { object: card }, &[], &[Answer::Bool(pay)]);
        let permanent = game.find(land).unwrap();
        assert_eq!(game.engine.state.objects[&permanent].tapped, tapped);
        assert_eq!(game.life(P0), after);
    }
}

#[test]
fn entering_tapped_does_not_trigger_becoming_tapped() {
    let mut table = Table::default();
    let card = table.card(
        "",
        "Creature",
        Some((2, 2)),
        "This creature enters tapped.\nWhenever this creature becomes tapped, you gain 1 life.",
    );
    let mut game = Game::new(table);
    let spell = game.put(card, P0, Zone::Hand);
    game.main();
    game.cast(spell, &[]);
    assert_eq!(
        game.life(P0),
        20,
        "CR 603.2e: entering tapped is not becoming tapped"
    );
}

#[test]
fn external_enter_tapped_effect_applies_to_face_down_creatures() {
    let mut table = Table::default();
    let authority = table.card(
        "",
        "Enchantment",
        None,
        "Creatures your opponents control enter tapped.",
    );
    let morph = table.card("", "Creature", Some((3, 3)), "Morph {1}");
    let mut game = Game::new(table);
    game.lands(3);
    game.put(authority, P1, Zone::Battlefield);
    let spell = game.put(morph, P0, Zone::Hand);
    game.main();
    game.act(Action::CastFaceDown { object: spell }, &[], &[]);
    let permanent = game.find(morph).unwrap();
    assert!(game.engine.state.objects[&permanent].face_down);
    assert!(game.engine.state.objects[&permanent].tapped);
}

#[test]
fn enter_as_copy_applies_the_copied_enter_tapped_ability() {
    let mut table = Table::default();
    let tapped = table.card("", "Creature", Some((3, 3)), "This creature enters tapped.");
    let clone = table.card(
        "",
        "Creature",
        Some((0, 0)),
        "You may have this creature enter as a copy of any creature on the battlefield.",
    );
    let mut game = Game::new(table);
    let original = game.put(tapped, P0, Zone::Battlefield);
    let spell = game.put(clone, P0, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![original])],
    );
    let copy = game
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| *id != original && game.engine.state.objects[id].card == tapped)
        .unwrap();
    assert!(game.engine.state.objects[&copy].tapped);
}
