//! Plain split cards through compiler, engine, projected views and bot.
use super::harness::*;
use mtg_core::{Color, Step, Target, Zone};
use mtg_engine::{Progress, actions::Action, choice::Answer, view::project};
use mtg_ir::Layout;

fn cards() -> (Table, mtg_core::CardId) {
    let mut t = Table::default();
    let left = t.card("{1}{U}", "Sorcery", None, "You gain 4 life.");
    let right = t.card(
        "{2}{R}",
        "Instant",
        None,
        "~ deals 3 damage to target creature.",
    );
    t.pair(left, right, Layout::Split);
    (t, left)
}

fn aftermath_cards() -> (Table, mtg_core::CardId) {
    let mut t = Table::default();
    let left = t.card("{R}", "Sorcery", None, "You gain 2 life.");
    let right = t.card("{1}{R}", "Sorcery", None,
        "Aftermath (Cast this spell only from your graveyard. Then exile it.)\n~ deals 3 damage to target creature.");
    t.pair(left, right, Layout::Split);
    (t, left)
}

#[test]
fn aftermath_is_graveyard_only_and_uses_printed_cost_and_timing() {
    let (mut t, card) = aftermath_cards();
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let victim = g.put(bear, P1, Zone::Battlefield);
    let hand = g.put(card, P0, Zone::Hand);
    let grave = g.put(card, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object: hand }));
    assert!(!actions.contains(&Action::CastFace {
        object: hand,
        face: 1
    }));
    assert!(!actions.contains(&Action::Cast { object: grave }));
    assert!(actions.contains(&Action::CastFace {
        object: grave,
        face: 1
    }));
    assert!(!g.until(P0, Step::BeginCombat).contains(&Action::CastFace {
        object: grave,
        face: 1
    }));
    g.until(P0, Step::PostcombatMain);
    g.act(
        Action::CastFace {
            object: grave,
            face: 1,
        },
        &[Target::Object(victim)],
        &[],
    );
    assert!(!g.engine.state.objects.contains_key(&victim));
    let exiled = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == card && o.zone.zone == Zone::Exile)
        .unwrap();
    assert_eq!(exiled.face, 0);
    assert_eq!(exiled.adventure_player, None);
}

#[test]
fn aftermath_requires_enough_mana_for_its_own_half() {
    let (mut t, card) = aftermath_cards();
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(bear, P1, Zone::Battlefield);
    let object = g.put(card, P0, Zone::Graveyard);
    assert!(!g.main().contains(&Action::CastFace { object, face: 1 }));
}

#[test]
fn aftermath_exiles_when_countered_or_all_targets_become_illegal() {
    for countered in [true, false] {
        let (mut t, card) = aftermath_cards();
        let bear = t.bear();
        let response = t.card(
            "{R}",
            "Instant",
            None,
            if countered {
                "Counter target spell."
            } else {
                "Destroy target creature."
            },
        );
        let mut g = Game::new(t);
        g.lands(3);
        let victim = g.put(bear, P1, Zone::Battlefield);
        let object = g.put(card, P0, Zone::Graveyard);
        let response = g.put(response, P0, Zone::Hand);
        g.main();
        g.act_holding(
            Action::CastFace { object, face: 1 },
            &[Target::Object(victim)],
        );
        let target = if countered { g.stack()[0] } else { victim };
        g.act(
            Action::Cast { object: response },
            &[Target::Object(target)],
            &[],
        );
        let exiled = g
            .engine
            .state
            .objects
            .values()
            .find(|o| o.card == card)
            .unwrap();
        assert_eq!(exiled.zone.zone, Zone::Exile);
        assert_eq!(exiled.face, 0);
        assert_eq!(exiled.adventure_player, None);
    }
}

#[test]
fn casting_the_normal_half_does_not_exile_the_card() {
    let (t, card) = aftermath_cards();
    let mut g = Game::new(t);
    g.lands(1);
    let object = g.put(card, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object }, &[], &[]);
    assert_eq!(g.life(P0), 22);
    assert_eq!(
        g.engine
            .state
            .objects
            .values()
            .find(|o| o.card == card)
            .unwrap()
            .zone
            .zone,
        Zone::Graveyard
    );
}

#[test]
fn off_stack_both_halves_have_combined_cost_colors_and_types() {
    use mtg_engine::eval::{CharacteristicsSource, PrintedChars};
    let (t, card) = cards();
    let mut g = Game::new(t);
    for zone in [Zone::Hand, Zone::Graveyard, Zone::Library, Zone::Exile] {
        let object = g.put(card, P0, zone);
        let printed = PrintedChars(&g.table)
            .characteristics(&g.engine.state, object)
            .unwrap();
        let actual = g.engine.characteristics(&g.table, object).unwrap();
        assert_eq!(printed, *actual);
        assert_eq!(actual.mana_cost.mana_value(), 5);
        assert!(actual.colors.contains(Color::Blue));
        assert!(actual.colors.contains(Color::Red));
        assert!(actual.card_types.contains(&mtg_core::CardType::Instant));
        assert!(actual.card_types.contains(&mtg_core::CardType::Sorcery));
        assert!(actual.name.contains(" // "));
    }
}

#[test]
fn left_half_uses_its_own_cost_and_effect_not_the_combined_five_mana() {
    let (t, card) = cards();
    let mut g = Game::new(t);
    g.lands(2);
    let object = g.put(card, P0, Zone::Hand);
    assert!(g.main().contains(&Action::Cast { object }));
    g.act_holding(Action::Cast { object }, &[]);
    let spell = g.stack()[0];
    let ch = g.engine.characteristics(&g.table, spell).unwrap();
    assert_eq!(ch.mana_cost.mana_value(), 2);
    assert!(ch.colors.contains(Color::Blue));
    assert!(!ch.colors.contains(Color::Red));
    assert_eq!(ch.card_types, vec![mtg_core::CardType::Sorcery]);
    g.main();
    assert_eq!(g.life(P0), 24);
    let grave = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == card)
        .unwrap()
        .id;
    assert_eq!(
        g.engine
            .characteristics(&g.table, grave)
            .unwrap()
            .mana_cost
            .mana_value(),
        5
    );
}

#[test]
fn right_half_has_its_own_timing_required_target_and_public_face() {
    let (mut t, card) = cards();
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let object = g.put(card, P0, Zone::Hand);
    assert!(!g.main().contains(&Action::CastFace { object, face: 1 }));
    let victim = g.put(bear, P1, Zone::Battlefield);
    let actions = g.until(P0, Step::BeginCombat);
    assert!(!actions.contains(&Action::Cast { object }));
    assert!(actions.contains(&Action::CastFace { object, face: 1 }));
    g.act_holding(
        Action::CastFace { object, face: 1 },
        &[Target::Object(victim)],
    );
    let spell = g.stack()[0];
    assert_eq!(project(&g.engine.state, P1).visible[&spell].face, 1);
    let ch = g.engine.characteristics(&g.table, spell).unwrap();
    assert_eq!(ch.mana_cost.mana_value(), 3);
    assert!(!ch.colors.contains(Color::Blue));
    assert_eq!(ch.card_types, vec![mtg_core::CardType::Instant]);
    g.until(P0, Step::PostcombatMain);
    assert!(!g.engine.state.objects.contains_key(&victim));
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn graveyard_casting_permission_is_specific_to_a_half() {
    let mut t = Table::default();
    let left = t.card("{U}", "Sorcery", None, "You gain 2 life.");
    let right = t.card("{4}{R}", "Sorcery", None, "You gain 3 life.\nFlashback {R}");
    t.pair(left, right, Layout::Split);
    let mut g = Game::new(t);
    g.lands(1);
    let object = g.put(left, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(!actions.contains(&Action::Cast { object }));
    assert!(actions.contains(&Action::CastFace { object, face: 1 }));
    g.act(Action::CastFace { object, face: 1 }, &[], &[]);
    assert_eq!(g.life(P0), 23);
    let exiled = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == left)
        .unwrap();
    assert_eq!(exiled.zone.zone, Zone::Exile);
    assert_eq!(exiled.face, 0);
    assert_eq!(exiled.adventure_player, None);
}

#[test]
fn bot_chooses_a_half_and_targets_its_effect() {
    let mut t = Table::default();
    let left = t.card("{R}", "Sorcery", None, "Target player gains 2 life.");
    let right = t.card(
        "{1}{R}",
        "Sorcery",
        None,
        "~ deals 3 damage to target player.",
    );
    t.pair(left, right, Layout::Split);
    let mut g = Game::new(t);
    g.lands(2);
    let object = g.put(left, P0, Zone::Hand);
    g.main();
    let bot = mtg_policy::bot::Bot::new(&g.table, [left]);
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
                if g.stack().is_empty() {
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
