use super::harness::*;
use mtg_core::{Keyword, PlayerId, Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
    state::PlayerState,
};

const P2: PlayerId = PlayerId(2);

fn add_opponent(g: &mut Game, poison: u32, departed: bool) {
    let mut player = PlayerState::new(P2, 20);
    player.poison = poison;
    player.has_lost = departed;
    g.engine.state.players.insert(P2, player);
    g.engine.state.turn_order.push(P2);
}

#[test]
fn corrupted_static_abilities_require_one_living_opponent_to_meet_the_threshold() {
    for (first, second, departed, enabled) in [
        (0, 0, false, false),
        (2, 2, false, false),
        (3, 0, false, true),
        (0, 3, false, true),
        (0, 3, true, false),
    ] {
        let mut t = Table::default();
        let skipper = t.card("{0}", "Creature", Some((2, 2)), "Flying\nCorrupted — As long as an opponent has three or more poison counters, this creature has deathtouch and lifelink.");
        let mut g = Game::new(t);
        add_opponent(&mut g, second, departed);
        g.engine.state.players.get_mut(&P0).unwrap().poison = 9;
        g.engine.state.players.get_mut(&P1).unwrap().poison = first;
        let source = g.put(skipper, P0, Zone::Battlefield);
        g.main();
        assert!(g.has(source, Keyword::Flying));
        assert_eq!(g.has(source, Keyword::Deathtouch), enabled);
        assert_eq!(g.has(source, Keyword::Lifelink), enabled);
    }
}

#[test]
fn corrupted_static_abilities_update_when_poison_changes_or_an_opponent_leaves() {
    let mut t = Table::default();
    let skipper = t.card("{0}", "Creature", Some((2, 2)), "Flying\nCorrupted — As long as an opponent has three or more poison counters, this creature has deathtouch and lifelink.");
    let poison = t.card(
        "{0}",
        "Instant",
        None,
        "Target opponent gets a poison counter.",
    );
    let lethal = t.card(
        "{0}",
        "Instant",
        None,
        "Target opponent gets seven poison counters.",
    );
    let mut g = Game::new(t);
    add_opponent(&mut g, 0, false);
    g.engine.state.players.get_mut(&P1).unwrap().poison = 2;
    let source = g.put(skipper, P0, Zone::Battlefield);
    let poison = g.put(poison, P0, Zone::Hand);
    let lethal = g.put(lethal, P0, Zone::Hand);
    g.main();
    assert!(!g.has(source, Keyword::Lifelink));
    g.cast(poison, &[Target::Player(P1)]);
    assert!(g.has(source, Keyword::Lifelink));
    assert!(g.has(source, Keyword::Deathtouch));
    g.cast(lethal, &[Target::Player(P1)]);
    assert!(g.engine.state.player(P1).has_lost);
    assert!(!g.has(source, Keyword::Lifelink));
    assert!(!g.has(source, Keyword::Deathtouch));
    assert!(g.has(source, Keyword::Flying));
}

#[test]
fn poisoned_requires_one_counter_and_enables_infect_damage() {
    for poison in [0, 1] {
        let mut t = Table::default();
        let betrayers = t.card(
            "{0}",
            "Creature",
            Some((3, 1)),
            "This creature has infect as long as an opponent is poisoned.",
        );
        let mut g = Game::new(t);
        g.engine.state.players.get_mut(&P1).unwrap().poison = poison;
        let source = g.put(betrayers, P0, Zone::Battlefield);
        g.main();
        g.combat(&[source], &[], &[], &[]);
        assert_eq!(g.life(P1), if poison == 0 { 17 } else { 20 });
        assert_eq!(
            g.engine.state.player(P1).poison,
            if poison == 0 { 0 } else { 4 }
        );
    }
}

#[test]
fn distorted_curiosity_reduction_is_available_only_at_three_poison() {
    for poison in [2, 3] {
        let mut t = Table::default();
        let curiosity = t.card("{2}{U}", "Sorcery", None, "Corrupted — This spell costs {2} less to cast if an opponent has three or more poison counters.\nDraw two cards.");
        let mut g = Game::new(t);
        g.lands(1);
        g.engine.state.players.get_mut(&P1).unwrap().poison = poison;
        let spell = g.put(curiosity, P0, Zone::Hand);
        let actions = g.main();
        assert_eq!(
            actions.contains(&Action::Cast { object: spell }),
            poison == 3
        );
        if poison == 3 {
            let hand = g.count(Zone::Hand, P0);
            g.cast(spell, &[]);
            assert_eq!(g.count(Zone::Hand, P0), hand + 1);
        }
    }
}

#[test]
fn glistening_sphere_checks_corrupted_for_manual_activation_and_mana_planning() {
    for (poison, floating) in [(2, false), (3, false), (3, true)] {
        let mut t = Table::default();
        let sphere = t.card("{0}", "Artifact", None, "This artifact enters tapped.\nWhen this artifact enters, proliferate.\n{T}: Add one mana of any color.\nCorrupted — {T}: Add three mana of any one color. Activate only if an opponent has three or more poison counters.");
        let spell = t.card("{3}", "Sorcery", None, "You gain 1 life.");
        let mut g = Game::new(t);
        g.engine.state.players.get_mut(&P1).unwrap().poison = poison;
        let source = g.put(sphere, P0, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        let actions = g.main();
        let activation = Action::ActivateManaAbility {
            source,
            ability: mtg_core::AbilityId(3),
            color: Some(mtg_core::Color::Blue),
        };
        let ChoiceKind::Priority { legal } = &g.pending.as_ref().unwrap().kind else {
            panic!("expected priority");
        };
        assert_eq!(legal.mana_abilities.contains(&activation), poison == 3);
        assert_eq!(
            actions.contains(&Action::Cast { object: spell }),
            poison == 3
        );
        if poison == 3 {
            if floating {
                g.act(activation, &[], &[]);
                assert_eq!(g.engine.state.player(P0).mana.total(), 3);
            }
            g.cast(spell, &[]);
            assert_eq!(g.engine.state.player(P0).mana.total(), 0);
            assert_eq!(g.life(P0), 21);
        }
    }
}

#[test]
fn vivisection_evangelist_rechecks_corrupted_after_the_poisoned_opponent_leaves() {
    for remaining_poison in [0, 3] {
        let mut t = Table::default();
        let evangelist = t.card("{0}", "Creature", Some((4, 4)), "Vigilance\nCorrupted — When this creature enters, if an opponent has three or more poison counters, destroy target creature or planeswalker an opponent controls.");
        let poison = t.card(
            "{0}",
            "Instant",
            None,
            "Target opponent gets seven poison counters.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        add_opponent(&mut g, remaining_poison, false);
        g.engine.state.players.get_mut(&P1).unwrap().poison = 3;
        let target = g.put(bear, P2, Zone::Battlefield);
        let spell = g.put(evangelist, P0, Zone::Hand);
        let poison = g.put(poison, P0, Zone::Hand);
        g.main();
        g.act_holding(Action::Cast { object: spell }, &[]);
        let question = g.pending.take().unwrap();
        g.engine
            .answer(&g.table, question.id, Answer::Pass)
            .unwrap();
        g.drive(
            |_, question| match question.kind {
                ChoiceKind::ChooseTargets { .. } => {
                    Some(Answer::Targets(vec![vec![Target::Object(target)]]))
                }
                _ => None,
            },
            |g| g.find(evangelist).is_some() && !g.stack().is_empty(),
        );
        g.cast(poison, &[Target::Player(P1)]);
        assert!(g.engine.state.player(P1).has_lost);
        assert_eq!(
            g.engine.state.objects.contains_key(&target),
            remaining_poison == 0
        );
    }
}

#[test]
fn vivisection_evangelist_does_not_trigger_below_the_poison_threshold() {
    let mut t = Table::default();
    let evangelist = t.card("{0}", "Creature", Some((4, 4)), "Vigilance\nCorrupted — When this creature enters, if an opponent has three or more poison counters, destroy target creature or planeswalker an opponent controls.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.engine.state.players.get_mut(&P1).unwrap().poison = 2;
    let target = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(evangelist, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    assert!(g.engine.state.objects.contains_key(&target));
}
