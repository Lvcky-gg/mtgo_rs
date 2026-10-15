use super::harness::*;
use mtg_core::{Event, Keyword, Step, Target, Zone};
use mtg_engine::choice::Answer;

#[test]
fn soothsayers_or_condition_checks_events_rather_than_net_life_change() {
    for (effect, enabled) in [
        ("You gain 0 life.", false),
        ("You gain 2 life.", true),
        ("You lose 2 life.", true),
        ("You lose 2 life. You gain 2 life.", true),
        ("Each opponent loses 2 life.", false),
    ] {
        let mut t = Table::default();
        let soothsayer = t.card("{0}", "Creature", Some((2, 3)), "Flying\nAt the beginning of your end step, if you gained or lost life this turn, surveil 1.");
        let spell = t.card("{0}", "Instant", None, effect);
        let mut g = Game::new(t);
        g.put(soothsayer, P0, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        g.cast(spell, &[]);
        g.until(P0, Step::End);
        let scried = g.engine.log.iter().filter(|entry| matches!(entry.event, Event::Scried { player, surveil: true } if player == P0)).count();
        assert_eq!(scried, usize::from(enabled), "{effect}");
    }
}

#[test]
fn lunar_convocations_and_condition_requires_both_events_even_when_life_returns_to_twenty() {
    for (effect, enabled) in [
        ("You gain 2 life.", false),
        ("You lose 2 life.", false),
        ("You lose 2 life. You gain 2 life.", true),
        ("You gain 2 life. You lose 2 life.", true),
    ] {
        let mut t = Table::default();
        let convocation = t.card("{0}", "Enchantment", None, "At the beginning of your end step, if you gained and lost life this turn, create a 1/1 black Bat creature token with flying.");
        let spell = t.card("{0}", "Instant", None, effect);
        let mut g = Game::new(t);
        g.put(convocation, P0, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        g.cast(spell, &[]);
        g.until(P0, Step::End);
        let tokens: Vec<_> = g
            .engine
            .state
            .battlefield()
            .into_iter()
            .filter(|id| g.engine.state.objects[id].is_token)
            .collect();
        assert_eq!(tokens.len(), usize::from(enabled), "{effect}");
        if enabled {
            assert!(g.has(tokens[0], Keyword::Flying));
            assert_eq!(g.life(P0), 20);
        }
    }
}

#[test]
fn essence_channelers_loss_condition_survives_a_gain_and_resets_next_turn() {
    let mut t = Table::default();
    let channeler = t.card(
        "{0}",
        "Creature",
        Some((2, 2)),
        "As long as you've lost life this turn, this creature has flying and vigilance.",
    );
    let lose = t.card("{0}", "Instant", None, "You lose 2 life.");
    let gain = t.card("{0}", "Instant", None, "You gain 2 life.");
    let mut g = Game::new(t);
    let channeler = g.put(channeler, P0, Zone::Battlefield);
    let lose = g.put(lose, P0, Zone::Hand);
    let gain = g.put(gain, P0, Zone::Hand);
    g.main();
    assert!(!g.has(channeler, Keyword::Flying));
    g.cast(lose, &[]);
    assert!(g.has(channeler, Keyword::Flying));
    g.cast(gain, &[]);
    assert_eq!(g.life(P0), 20);
    assert!(g.has(channeler, Keyword::Flying));
    assert!(g.has(channeler, Keyword::Vigilance));
    g.until(P1, Step::PrecombatMain);
    assert!(!g.has(channeler, Keyword::Flying));
    assert!(!g.has(channeler, Keyword::Vigilance));
}

#[test]
fn groundling_pouncer_requires_an_opponents_flyer_and_retains_its_once_per_turn_limit() {
    for opponent_has_flyer in [false, true] {
        let mut t = Table::default();
        let pouncer = t.card("{0}", "Creature", Some((2, 1)), "{G/U}: This creature gets +1/+3 and gains flying until end of turn. Activate only once each turn and only if an opponent controls a creature with flying.");
        let flyer = t.card("{0}", "Creature", Some((1, 1)), "Flying");
        let mana = t.card("{0}", "Instant", None, "Add {G}.");
        let mut g = Game::new(t);
        g.lands(1);
        let pouncer = g.put(pouncer, P0, Zone::Battlefield);
        let mana = g.put(mana, P0, Zone::Hand);
        g.put(
            flyer,
            if opponent_has_flyer { P1 } else { P0 },
            Zone::Battlefield,
        );
        let actions = g.main();
        assert_eq!(actions.contains(&activate(pouncer, 0)), opponent_has_flyer);
        if opponent_has_flyer {
            g.act(activate(pouncer, 0), &[], &[]);
            assert_eq!(g.pt(pouncer), (3, 4));
            assert!(g.has(pouncer, Keyword::Flying));
            // Supply mana for another activation so its absence proves the limit.
            g.cast(mana, &[]);
            assert_eq!(g.engine.state.player(P0).mana.total(), 1);
            assert!(!g.main().contains(&activate(pouncer, 0)));
        }
    }
}

#[test]
fn dread_wanderer_requires_a_small_hand_and_sorcery_timing() {
    let mut t = Table::default();
    let wanderer = t.card("{0}", "Creature", Some((2, 1)), "This creature enters tapped.\n{2}{B}: Return this card from your graveyard to the battlefield. Activate only as a sorcery and only if you have one or fewer cards in hand.");
    let instant = t.card("{0}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(3);
    let source = g.put(wanderer, P0, Zone::Graveyard);
    let instant = g.put(instant, P0, Zone::Hand);
    assert!(!g.main().contains(&activate(source, 1)));
    assert_eq!(g.count(Zone::Hand, P0), 2);
    g.act_holding(mtg_engine::actions::Action::Cast { object: instant }, &[]);
    assert_eq!(g.count(Zone::Hand, P0), 1);
    let mtg_engine::choice::ChoiceKind::Priority { legal } = &g.pending.as_ref().unwrap().kind
    else {
        panic!("expected priority");
    };
    assert!(!legal.actions.contains(&activate(source, 1)));
    let question = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, question.id, Answer::Pass)
        .unwrap();
    g.drive(|_, _| None, |g| g.stack().is_empty());
    assert!(g.main().contains(&activate(source, 1)));
    g.act(activate(source, 1), &[], &[]);
    let returned = g.find(wanderer).unwrap();
    assert!(g.engine.state.objects[&returned].tapped);
}

#[test]
fn gutterbones_requires_both_its_owners_turn_and_an_opponents_life_loss() {
    for active in [P0, P1] {
        let mut t = Table::default();
        let gutterbones = t.card("{0}", "Creature", Some((2, 1)), "This creature enters tapped.\n{1}{B}: Return this card from your graveyard to your hand. Activate only during your turn and only if an opponent lost life this turn.");
        let loss = t.card("{0}", "Instant", None, "Target opponent loses 1 life.");
        let mut g = Game::new(t);
        g.lands(2);
        let source = g.put(gutterbones, P0, Zone::Graveyard);
        let loss = g.put(loss, P0, Zone::Hand);
        g.until(active, Step::PrecombatMain);
        if active == P1 {
            let question = g.pending.take().unwrap();
            g.engine
                .answer(&g.table, question.id, Answer::Pass)
                .unwrap();
            g.drive(|_, _| None, |g| g.engine.state.priority == Some(P0));
        }
        let mtg_engine::choice::ChoiceKind::Priority { legal } = &g.pending.as_ref().unwrap().kind
        else {
            panic!("expected priority");
        };
        assert!(!legal.actions.contains(&activate(source, 1)));
        g.cast(loss, &[Target::Player(P1)]);
        let mtg_engine::choice::ChoiceKind::Priority { legal } = &g.pending.as_ref().unwrap().kind
        else {
            panic!("expected priority");
        };
        assert_eq!(legal.actions.contains(&activate(source, 1)), active == P0);
        if active == P0 {
            g.act(activate(source, 1), &[], &[]);
            assert!(
                g.engine
                    .state
                    .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
                    .iter()
                    .any(|id| g.engine.state.objects[id].card == gutterbones)
            );
        }
    }
}
