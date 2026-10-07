//! "Any number of target …" (a run of optional slots) and strive (CR 702.103).

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::Progress;
use mtg_engine::actions::Action;
use mtg_engine::choice::{Answer, ChoiceKind};

fn tapped(g: &Game) -> usize {
    g.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].tapped)
        .count()
}

#[test]
fn any_number_of_targets_takes_as_many_as_chosen() {
    for n in [1usize, 3] {
        let mut t = Table::default();
        let wipe = t.card(
            "{W}",
            "Sorcery",
            None,
            "Exile any number of target creatures.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let bears: Vec<_> = (0..4).map(|_| g.put(bear, P1, Zone::Battlefield)).collect();
        let s = g.put(wipe, P0, Zone::Hand);
        g.main();
        let targets: Vec<Target> = bears[..n].iter().map(|b| Target::Object(*b)).collect();
        g.act(Action::Cast { object: s }, &targets, &[]);
        let left = bears
            .iter()
            .filter(|b| g.engine.state.objects.contains_key(b))
            .count();
        assert_eq!(left, 4 - n, "{n} chosen");
    }
}

#[test]
fn every_creature_can_be_targeted_and_none_twice() {
    let mut t = Table::default();
    let wipe = t.card(
        "{W}",
        "Sorcery",
        None,
        "Return any number of target creatures to their owners' hands.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let a = g.put(bear, P1, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(wipe, P0, Zone::Hand);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, c.id, Answer::Action(Action::Cast { object: s }))
        .unwrap();
    let mut offered = Vec::new();
    for _ in 0..1000 {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::ChooseTargets { slots, .. } => {
                    offered.push(slots[0].clone());
                    let pick = slots[0][0];
                    g.engine
                        .answer(&g.table, c.id, Answer::Targets(vec![vec![pick]]))
                        .unwrap();
                }
                ChoiceKind::Priority { .. } if g.stack().is_empty() => break,
                _ => {
                    let d = c.default.clone().unwrap();
                    g.engine.answer(&g.table, c.id, d).unwrap();
                }
            },
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!(),
        }
    }
    assert_eq!(
        offered.len(),
        2,
        "asked until no creature was left: {offered:?}"
    );
    assert!(
        !offered[1].contains(&offered[0][0]),
        "never the same one twice"
    );
    assert!(!g.engine.state.objects.contains_key(&a));
    assert!(!g.engine.state.objects.contains_key(&b));
}

#[test]
fn strive_costs_more_for_each_target_beyond_the_first() {
    for n in [1usize, 3] {
        let mut t = Table::default();
        let rally = t.card(
            "{R}",
            "Instant",
            None,
            "Strive — This spell costs {1}{R} more to cast for each target beyond the first.\n\
             Any number of target creatures each get +2/+0 until end of turn.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(6);
        let bears: Vec<_> = (0..3).map(|_| g.put(bear, P0, Zone::Battlefield)).collect();
        let s = g.put(rally, P0, Zone::Hand);
        g.main();
        let targets: Vec<Target> = bears[..n].iter().map(|b| Target::Object(*b)).collect();
        g.act(Action::Cast { object: s }, &targets, &[]);
        assert_eq!(tapped(&g), 1 + 2 * (n - 1), "{n} targets");
        for (i, b) in bears.iter().enumerate() {
            assert_eq!(g.pt(*b).0, if i < n { 4 } else { 2 });
        }
    }
}

#[test]
fn strive_offers_no_target_it_cannot_pay_for() {
    let mut t = Table::default();
    let rally = t.card(
        "{R}",
        "Instant",
        None,
        "Strive — This spell costs {1}{R} more to cast for each target beyond the first.\n\
         Any number of target creatures each get +2/+0 until end of turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    // {R} and one more {1}{R}: two targets at most.
    g.lands(4);
    let bears: Vec<_> = (0..4).map(|_| g.put(bear, P0, Zone::Battlefield)).collect();
    let s = g.put(rally, P0, Zone::Hand);
    g.main();
    // Asking for a third target would panic the harness: it is never offered.
    let targets = [Target::Object(bears[0]), Target::Object(bears[1])];
    g.act(Action::Cast { object: s }, &targets, &[]);
    assert_eq!(tapped(&g), 3);
    assert_eq!(g.pt(bears[1]).0, 4);
    assert_eq!(g.pt(bears[2]).0, 2);
}

#[test]
fn and_or_names_either_type() {
    let mut t = Table::default();
    let shatter = t.card(
        "{G}",
        "Instant",
        None,
        "Destroy up to two target artifacts and/or enchantments.",
    );
    let rock = t.card("{1}", "Artifact", None, "");
    let charm = t.card("{1}", "Enchantment", None, "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let r = g.put(rock, P1, Zone::Battlefield);
    let c = g.put(charm, P1, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    let s = g.put(shatter, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: s },
        &[Target::Object(r), Target::Object(c)],
        &[],
    );
    assert!(!g.engine.state.objects.contains_key(&r));
    assert!(!g.engine.state.objects.contains_key(&c));
}

#[test]
fn those_creatures_are_the_targets_just_named() {
    let mut t = Table::default();
    let rally = t.card(
        "{G}",
        "Instant",
        None,
        "Any number of target creatures each get +2/+2 until end of turn. Untap those \
         creatures.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let a = g.put(bear, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let c = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(rally, P0, Zone::Hand);
    g.main();
    for id in [a, b, c] {
        g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
    }
    g.act(
        Action::Cast { object: s },
        &[Target::Object(a), Target::Object(b)],
        &[],
    );
    assert_eq!(g.pt(a), (4, 4));
    assert_eq!(g.pt(b), (4, 4));
    assert_eq!(g.pt(c), (2, 2));
    assert!(!g.engine.state.objects[&a].tapped);
    assert!(!g.engine.state.objects[&b].tapped);
    assert!(g.engine.state.objects[&c].tapped, "not a target");
}

#[test]
fn two_target_players_each_discard() {
    let mut t = Table::default();
    let mind = t.card(
        "{B}",
        "Sorcery",
        None,
        "Two target players each discard a card.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(bear, P0, Zone::Hand);
    g.put(bear, P1, Zone::Hand);
    let s = g.put(mind, P0, Zone::Hand);
    g.main();
    let (mine, theirs) = (g.count(Zone::Hand, P0), g.count(Zone::Hand, P1));
    g.act(
        Action::Cast { object: s },
        &[Target::Player(P0), Target::Player(P1)],
        &[],
    );
    assert_eq!(
        g.count(Zone::Hand, P0),
        mine - 2,
        "the spell, then a discard"
    );
    assert_eq!(g.count(Zone::Hand, P1), theirs - 1);
}

#[test]
fn any_number_of_target_opponents_each_lose_life() {
    let mut t = Table::default();
    let drain = t.card(
        "{B}",
        "Sorcery",
        None,
        "Any number of target opponents each lose 2 life.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(drain, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: s }, &[Target::Player(P1)], &[]);
    assert_eq!(g.life(P1), 18);
    assert_eq!(g.life(P0), 20);
}

#[test]
fn x_damage_divided_takes_x_shares() {
    let mut t = Table::default();
    let slide = t.card(
        "{X}{R}",
        "Sorcery",
        None,
        "~ deals X damage divided as you choose among any number of target creatures.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    let a = g.put(bear, P1, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(slide, P0, Zone::Hand);
    g.main();
    // The harness would panic if a fourth share were asked for.
    g.act(
        Action::Cast { object: s },
        &[Target::Object(a), Target::Object(a), Target::Object(b)],
        &[Answer::Number(3)],
    );
    assert!(!g.engine.state.objects.contains_key(&a), "two damage");
    assert_eq!(g.engine.state.objects[&b].damage, 1);
    assert_eq!(tapped(&g), 4);
}

#[test]
fn from_a_single_graveyard_keeps_to_the_first_targets_owner() {
    let mut t = Table::default();
    let purge = t.card(
        "{B}",
        "Sorcery",
        None,
        "Exile up to three target cards from a single graveyard.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let theirs1 = g.put(bear, P1, Zone::Graveyard);
    let theirs2 = g.put(bear, P1, Zone::Graveyard);
    let mine = g.put(bear, P0, Zone::Graveyard);
    let s = g.put(purge, P0, Zone::Hand);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, c.id, Answer::Action(Action::Cast { object: s }))
        .unwrap();
    let mut offered = Vec::new();
    for _ in 0..1000 {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::ChooseTargets { slots, .. } => {
                    offered.push(slots[0].clone());
                    let pick = if offered.len() == 1 {
                        Target::Object(theirs1)
                    } else {
                        slots[0][0]
                    };
                    g.engine
                        .answer(&g.table, c.id, Answer::Targets(vec![vec![pick]]))
                        .unwrap();
                }
                ChoiceKind::Priority { .. } if g.stack().is_empty() => break,
                _ => {
                    let d = c.default.clone().unwrap();
                    g.engine.answer(&g.table, c.id, d).unwrap();
                }
            },
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!(),
        }
    }
    assert!(offered[0].contains(&Target::Object(mine)), "any graveyard at first");
    assert_eq!(offered[1], vec![Target::Object(theirs2)], "then only theirs");
    assert!(g.engine.state.objects.contains_key(&mine));
    assert!(!g.engine.state.objects.contains_key(&theirs2));
}
