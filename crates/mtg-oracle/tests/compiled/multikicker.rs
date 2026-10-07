//! Multikicker and replicate: a kicker paid any number of times (CR 702.33c, 702.56).

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;
use mtg_engine::choice::Answer;

fn tapped(g: &Game) -> usize {
    g.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].tapped)
        .count()
}

#[test]
fn multikicker_counts_each_payment() {
    for (times, pt) in [(0, (1, 1)), (1, (2, 2)), (3, (4, 4))] {
        let mut t = Table::default();
        let elf = t.card(
            "{G}",
            "Creature — Elf",
            Some((1, 1)),
            "Multikicker {1}\nThis creature enters with a +1/+1 counter on it for each time it \
             was kicked.",
        );
        let mut g = Game::new(t);
        g.lands(4);
        let e = g.put(elf, P0, Zone::Hand);
        g.main();
        g.act(Action::Cast { object: e }, &[], &[Answer::Number(times)]);
        let id = g.find(elf).expect("on the battlefield");
        assert_eq!(g.pt(id), pt, "kicked {times} times");
        // {G} plus {1} per kick.
        assert_eq!(tapped(&g), 1 + times as usize);
    }
}

#[test]
fn multikicker_is_offered_only_as_many_times_as_can_be_paid() {
    use mtg_engine::Progress;
    use mtg_engine::choice::ChoiceKind;
    let mut t = Table::default();
    let elf = t.card(
        "{G}",
        "Creature — Elf",
        Some((1, 1)),
        "Multikicker {1}{G}\nThis creature enters with a +1/+1 counter on it for each time it \
         was kicked.",
    );
    let mut g = Game::new(t);
    g.lands(6);
    let e = g.put(elf, P0, Zone::Hand);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, c.id, Answer::Action(Action::Cast { object: e }))
        .unwrap();
    let choice = loop {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) => break c,
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game over"),
        }
    };
    // Six lands: {G} and two kicks of {1}{G}, with one land left over.
    assert!(
        matches!(choice.kind, ChoiceKind::ChooseX { min: 0, max: 2 }),
        "{:?}",
        choice.kind
    );
    assert!(
        g.engine
            .answer(&g.table, choice.id, Answer::Number(3))
            .is_err(),
        "a third kick cannot be paid"
    );
}

#[test]
fn multikicker_trigger_makes_a_token_per_kick() {
    let mut t = Table::default();
    let druid = t.card(
        "{G}",
        "Creature — Elf",
        Some((1, 1)),
        "Multikicker {1}\nWhen this creature enters, create a 2/2 green Elf creature token \
         for each time it was kicked.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let d = g.put(druid, P0, Zone::Hand);
    g.main();
    let before = g.engine.state.battlefield().len();
    g.act(Action::Cast { object: d }, &[], &[Answer::Number(2)]);
    assert_eq!(
        g.engine.state.battlefield().len(),
        before + 3,
        "it and two wolves"
    );
}

#[test]
fn replicate_copies_the_spell_once_per_payment() {
    for times in [0u32, 2] {
        let mut t = Table::default();
        let shock = t.card(
            "{R}",
            "Instant",
            None,
            "Replicate {1}\n~ deals 1 damage to target player.",
        );
        let mut g = Game::new(t);
        g.lands(3);
        let s = g.put(shock, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: s },
            &[Target::Player(P1); 3],
            &[Answer::Number(times)],
        );
        assert_eq!(
            g.life(P1),
            20 - 1 - times as i32,
            "replicated {times} times"
        );
    }
}

#[test]
fn a_kicker_is_paid_for() {
    let mut t = Table::default();
    let elf = t.card(
        "{G}",
        "Creature — Elf",
        Some((1, 1)),
        "Kicker {1}\nIf this creature was kicked, it enters with two +1/+1 counters on it.",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let e = g.put(elf, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: e }, &[], &[Answer::Bool(true)]);
    let id = g.find(elf).expect("on the battlefield");
    assert_eq!(g.pt(id), (3, 3));
    assert_eq!(tapped(&g), 2, "{{G}} and the kicker's {{1}}");
}

#[test]
fn a_token_for_each_creature_you_control() {
    let mut t = Table::default();
    let call = t.card(
        "{G}",
        "Sorcery",
        None,
        "Create a 1/1 green Elf creature token for each creature you control.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    let s = g.put(call, P0, Zone::Hand);
    g.main();
    let before = g.engine.state.battlefield().len();
    g.act(Action::Cast { object: s }, &[], &[]);
    assert_eq!(g.engine.state.battlefield().len(), before + 2);
}

#[test]
fn escalate_costs_more_for_each_mode_beyond_the_first() {
    use mtg_engine::Progress;
    use mtg_engine::choice::ChoiceKind;
    let text = "Escalate {1}\nChoose one or more —\n• You gain 1 life.\n• You gain 2 life.\n\
                • You gain 4 life.";
    // Three lands: {W} and two escalations, every mode.
    let mut t = Table::default();
    let charm = t.card("{W}", "Sorcery", None, text);
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(charm, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: s },
        &[],
        &[Answer::Modes(vec![0, 1, 2])],
    );
    assert_eq!(g.life(P0), 27);
    assert_eq!(tapped(&g), 3);

    // Two lands: at most two modes are offered.
    let mut t = Table::default();
    let charm = t.card("{W}", "Sorcery", None, text);
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(charm, P0, Zone::Hand);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, c.id, Answer::Action(Action::Cast { object: s }))
        .unwrap();
    let choice = loop {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) => break c,
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game over"),
        }
    };
    assert!(
        matches!(choice.kind, ChoiceKind::ChooseModes { count: 2, .. }),
        "{:?}",
        choice.kind
    );
}

#[test]
fn flash_for_more_is_paid_only_at_instant_speed() {
    use mtg_core::Step;
    let offered = |actions: &[Action], id| {
        actions
            .iter()
            .any(|a| matches!(a, Action::Cast { object } if *object == id))
    };
    let text = "You may cast this spell as though it had flash if you pay {2} more to cast it.";
    // In a main phase: the printed cost.
    let mut t = Table::default();
    let bear = t.card("{1}{G}", "Creature — Bear", Some((2, 2)), text);
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bear, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: b }, &[], &[]);
    assert!(
        g.find(bear)
            .is_some_and(|id| g.engine.state.objects[&id].zone.zone == Zone::Battlefield)
    );
    assert_eq!(tapped(&g), 2);

    // In the upkeep: {2} more, and not offered when that can't be paid.
    for (lands, castable) in [(3, false), (4, true)] {
        let mut t = Table::default();
        let bear = t.card("{1}{G}", "Creature — Bear", Some((2, 2)), text);
        let mut g = Game::new(t);
        g.lands(lands);
        let b = g.put(bear, P0, Zone::Hand);
        let actions = g.until(P0, Step::Upkeep);
        assert_eq!(offered(&actions, b), castable, "{lands} lands");
        if castable {
            g.act(Action::Cast { object: b }, &[], &[]);
            assert_eq!(tapped(&g), 4);
        }
    }
}

#[test]
fn flashed_in_it_is_sacrificed_at_the_next_cleanup() {
    use mtg_core::Step;
    let text = "You may cast this spell as though it had flash. If you cast it any time a \
                sorcery couldn't have been cast, the controller of the permanent it becomes \
                sacrifices it at the beginning of the next cleanup step.";
    for flashed in [true, false] {
        let mut t = Table::default();
        let bear = t.card("{1}{G}", "Creature — Bear", Some((2, 2)), text);
        let mut g = Game::new(t);
        g.lands(2);
        let b = g.put(bear, P0, Zone::Hand);
        if flashed {
            g.until(P0, Step::Upkeep);
        } else {
            g.main();
        }
        g.act(Action::Cast { object: b }, &[], &[]);
        let on_field = |g: &Game| {
            g.engine
                .state
                .battlefield()
                .iter()
                .any(|id| g.engine.state.objects[id].card == bear)
        };
        assert!(on_field(&g));
        g.until(P1, Step::Upkeep);
        assert_eq!(on_field(&g), !flashed, "flashed: {flashed}");
    }
}

#[test]
fn emerge_sacrifices_a_creature_and_costs_its_mana_value_less() {
    use mtg_engine::Progress;
    use mtg_engine::choice::ChoiceKind;
    let mut t = Table::default();
    let titan = t.card("{7}{U}", "Creature — Elf", Some((8, 8)), "Emerge {5}{U}");
    let big = t.card("{4}", "Creature — Bear", Some((4, 4)), "");
    let small = t.card("{1}", "Creature — Bear", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(2);
    let x = g.put(titan, P0, Zone::Hand);
    let b = g.put(big, P0, Zone::Battlefield);
    let s = g.put(small, P0, Zone::Battlefield);
    let actions = g.main();
    let alt = actions
        .iter()
        .find(|a| matches!(a, Action::CastAlternative { object, .. } if *object == x))
        .cloned()
        .expect("emerge offered: {5}{U} less 4 is {1}{U}");
    // Only the 4-drop pays enough; the 1-drop is not offered.
    let c = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, c.id, Answer::Action(alt))
        .unwrap();
    let choice = loop {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) => break c,
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!(),
        }
    };
    let ChoiceKind::ChooseObjects { from, .. } = &choice.kind else {
        panic!("{:?}", choice.kind)
    };
    assert_eq!(from, &vec![b]);
    g.engine
        .answer(&g.table, choice.id, Answer::Objects(vec![b]))
        .unwrap();
    for _ in 0..100 {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) => {
                if matches!(c.kind, ChoiceKind::Priority { .. }) && g.stack().is_empty() {
                    break;
                }
                let d = c.default.clone().unwrap_or(Answer::Pass);
                g.engine.answer(&g.table, c.id, d).unwrap();
            }
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!(),
        }
    }
    assert!(!g.engine.state.objects.contains_key(&b), "sacrificed");
    assert!(g.engine.state.objects.contains_key(&s));
    assert!(
        g.engine
            .state
            .battlefield()
            .iter()
            .any(|id| g.engine.state.objects[id].card == titan)
    );
    assert_eq!(tapped(&g), 2);
}

#[test]
fn spree_pays_for_each_chosen_mode() {
    use mtg_core::Target;
    let text = "Spree (Choose one or more additional costs.)\n+ {1} — ~ deals 1 damage to \
                target player.\n+ {2} — You gain 3 life.";
    for (lands, their_life, my_life) in [(4, 19, 23), (2, 19, 20)] {
        let mut t = Table::default();
        let spell = t.card("{R}", "Sorcery", None, text);
        let mut g = Game::new(t);
        g.lands(lands);
        let s = g.put(spell, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: s },
            &[Target::Player(P1)],
            &[Answer::Modes(vec![0, 1])],
        );
        assert_eq!((g.life(P1), g.life(P0)), (their_life, my_life), "{lands} lands");
        assert_eq!(tapped(&g), lands);
    }
}

#[test]
fn squad_makes_a_token_copy_per_payment() {
    let mut t = Table::default();
    let trooper = t.card("{1}{W}", "Creature — Soldier", Some((2, 2)), "Squad {2}");
    let mut g = Game::new(t);
    g.lands(6);
    let s = g.put(trooper, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: s }, &[], &[Answer::Number(2)]);
    let troopers = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.pt(*id) == (2, 2))
        .count();
    assert_eq!(troopers, 3, "the card and two token copies");
    assert_eq!(tapped(&g), 6);
}

#[test]
fn ravenous_draws_only_at_five_or_more() {
    for (x, draws) in [(4u32, 0usize), (5, 1)] {
        let mut t = Table::default();
        let maw = t.card("{X}{G}", "Creature — Elf", Some((0, 0)), "Ravenous");
        let mut g = Game::new(t);
        g.lands(6);
        let m = g.put(maw, P0, Zone::Hand);
        g.main();
        let hand = g.count(Zone::Hand, P0);
        g.act(Action::Cast { object: m }, &[], &[Answer::Number(x)]);
        let id = g.find(maw).unwrap();
        assert_eq!(g.pt(id), (x as i32, x as i32));
        assert_eq!(g.count(Zone::Hand, P0), hand - 1 + draws, "X = {x}");
    }
}
