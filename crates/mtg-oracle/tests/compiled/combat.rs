//! Combat restrictions and triggers that look at who blocks whom.
use std::collections::BTreeMap;

use super::harness::*;
use mtg_core::{ObjectId, Step, Target, Zone};
use mtg_engine::{
    Progress,
    choice::{Answer, ChoiceKind},
};

/// Advance from P0's main phase to the attack declaration and offer `attackers`;
/// whether the engine accepted them.
fn try_attack(g: &mut Game, attackers: &[ObjectId]) -> bool {
    let c = g.pending.take().expect("no priority prompt");
    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
    loop {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::DeclareAttackers { .. } => {
                    return g
                        .engine
                        .answer(&g.table, c.id, Answer::Objects(attackers.to_vec()))
                        .is_ok();
                }
                _ => {
                    let a = c.default.clone().unwrap_or(Answer::Pass);
                    g.engine.answer(&g.table, c.id, a).unwrap();
                }
            },
            Progress::GameOver { .. } => panic!("game ended"),
        }
    }
}

fn blocks(
    g: &Game,
    pairs: &[(ObjectId, Vec<ObjectId>)],
) -> Result<(), mtg_engine::combat::BlockError> {
    let blocks: BTreeMap<_, _> = pairs.iter().cloned().collect();
    mtg_engine::combat::validate_blocks(&g.engine.state, &g.table, P1, &blocks)
}

#[test]
fn a_creature_that_cant_attack_alone_needs_company() {
    for company in [false, true] {
        let mut t = Table::default();
        let coward = t.card(
            "{1}{R}",
            "Creature — Goblin",
            Some((2, 2)),
            "This creature can't attack alone.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        let c = g.put(coward, P0, Zone::Battlefield);
        let b = g.put(bear, P0, Zone::Battlefield);
        g.main();
        let attackers = if company { vec![c, b] } else { vec![c] };
        assert_eq!(try_attack(&mut g, &attackers), company);
    }
}

#[test]
fn a_creature_that_cant_attack_or_block_alone_attacks_with_others_and_blocks_in_pairs() {
    let mut t = Table::default();
    let coward = t.card(
        "{1}{R}",
        "Creature — Goblin",
        Some((2, 2)),
        "This creature can't attack or block alone.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    let c = g.put(coward, P1, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.main();
    // Blocking legality is a property of the whole declaration.
    g.engine
        .state
        .combat
        .attackers
        .insert(attacker, Target::Player(P1));
    assert!(blocks(&g, &[(attacker, vec![c])]).is_err(), "alone");
    assert!(blocks(&g, &[(attacker, vec![b])]).is_ok());
    assert!(
        blocks(&g, &[(attacker, vec![c, b])]).is_ok(),
        "with company"
    );
}

#[test]
fn three_or_more_creatures_must_block_it() {
    let mut t = Table::default();
    let giant = t.card(
        "{4}{G}",
        "Creature — Bear",
        Some((5, 5)),
        "This creature can't be blocked except by three or more creatures.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let a = g.put(giant, P0, Zone::Battlefield);
    let bs: Vec<_> = (0..3).map(|_| g.put(bear, P1, Zone::Battlefield)).collect();
    g.main();
    g.engine
        .state
        .combat
        .attackers
        .insert(a, Target::Player(P1));
    assert!(blocks(&g, &[(a, bs[..1].to_vec())]).is_err());
    assert!(blocks(&g, &[(a, bs[..2].to_vec())]).is_err());
    assert!(blocks(&g, &[(a, bs.clone())]).is_ok());
}

#[test]
fn rampage_counts_blockers_beyond_the_first() {
    for blockers in [1usize, 3] {
        let mut t = Table::default();
        let beast = t.card("{3}{G}", "Creature — Bear", Some((3, 3)), "Rampage 2");
        let bear = t.bear();
        let mut g = Game::new(t);
        let a = g.put(beast, P0, Zone::Battlefield);
        let bs: Vec<_> = (0..blockers)
            .map(|_| g.put(bear, P1, Zone::Battlefield))
            .collect();
        g.main();
        let pairs: Vec<_> = bs.iter().map(|b| (*b, a)).collect();
        g.combat(&[a], &pairs, &[], &[]);
        if blockers == 1 {
            assert!(g.find(beast).is_some(), "a 3/3 survives one bear");
            assert_eq!(g.pt(a), (3, 3), "no bonus for the first blocker");
        } else {
            // +4/+4 for the two beyond the first: a 7/7 that took 6.
            assert_eq!(g.pt(a), (7, 7));
            assert_eq!(g.count(Zone::Graveyard, P1), 3, "7 damage killed all three");
        }
    }
}

#[test]
fn it_grows_for_each_creature_blocking_it() {
    let mut t = Table::default();
    let beast = t.card(
        "{3}{G}",
        "Creature — Bear",
        Some((3, 3)),
        "Whenever this creature becomes blocked, it gets +1/+1 until end of turn for each \
         creature blocking it.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let a = g.put(beast, P0, Zone::Battlefield);
    let b1 = g.put(bear, P1, Zone::Battlefield);
    let b2 = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.combat(&[a], &[(b1, a), (b2, a)], &[], &[]);
    assert_eq!(
        g.pt(a),
        (5, 5),
        "two blockers, +2/+2, so it survived their 4"
    );
    assert_eq!(g.count(Zone::Graveyard, P1), 2);
}

#[test]
fn a_block_trigger_that_cares_what_it_blocks() {
    for flier in [false, true] {
        let mut t = Table::default();
        let archer = t.card(
            "{1}{G}",
            "Creature — Elf",
            Some((1, 3)),
            "Reach\nWhenever this creature blocks a creature with flying, this creature gets \
             +2/+0 until end of turn.",
        );
        let attacker = t.card(
            "{1}{W}",
            "Creature — Spirit",
            Some((2, 2)),
            if flier { "Flying" } else { "" },
        );
        let mut g = Game::new(t);
        let a = g.put(attacker, P0, Zone::Battlefield);
        let b = g.put(archer, P1, Zone::Battlefield);
        g.main();
        g.combat(&[a], &[(b, a)], &[], &[]);
        assert_eq!(
            g.find(attacker).is_none(),
            flier,
            "only a flier meets the 3-power archer"
        );
        assert!(g.find(archer).is_some(), "the 3-toughness archer took 2");
    }
}

#[test]
fn that_creature_is_the_blocker_not_the_attacker() {
    let mut t = Table::default();
    let wurm = t.card(
        "{4}{G}",
        "Creature — Bear",
        Some((1, 1)),
        "Whenever this creature becomes blocked by a Wall, destroy that creature.",
    );
    let wall = t.card("{1}{W}", "Creature — Wall", Some((0, 4)), "Defender");
    let mut g = Game::new(t);
    let a = g.put(wurm, P0, Zone::Battlefield);
    let w = g.put(wall, P1, Zone::Battlefield);
    g.main();
    g.combat(&[a], &[(w, a)], &[], &[]);
    assert!(g.find(wall).is_none(), "the wall was destroyed");
    assert!(g.find(wurm).is_some(), "not the wurm");
}

#[test]
fn blocks_or_becomes_blocked_destroys_the_other_creature_at_end_of_combat() {
    for attacking in [true, false] {
        let mut t = Table::default();
        let basilisk = t.card(
            "{3}{B}",
            "Creature — Zombie",
            Some((1, 1)),
            "Whenever this creature blocks or becomes blocked by a creature, destroy that \
             creature at end of combat.",
        );
        let ogre = t.card("{3}{R}", "Creature — Goblin", Some((5, 5)), "");
        let mut g = Game::new(t);
        let (mine, theirs) = if attacking {
            (basilisk, ogre)
        } else {
            (ogre, basilisk)
        };
        let a = g.put(mine, P0, Zone::Battlefield);
        let b = g.put(theirs, P1, Zone::Battlefield);
        g.main();
        g.combat(&[a], &[(b, a)], &[], &[]);
        // The 1/1 dies to combat damage; the 5/5 to the delayed trigger.
        assert!(g.find(ogre).is_none(), "the other creature was destroyed");
        assert!(g.find(basilisk).is_none());
        assert_eq!(g.engine.state.step, Step::PostcombatMain);
    }
}

#[test]
fn a_spell_cast_only_during_combat() {
    let mut t = Table::default();
    let trick = t.card(
        "{W}",
        "Instant",
        None,
        "Cast this spell only during combat.\nYou gain 3 life.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(trick, P0, Zone::Hand);
    let cast = mtg_engine::actions::Action::Cast { object: s };
    assert!(!g.main().contains(&cast), "not in the main phase");
    assert!(g.until(P0, Step::BeginCombat).contains(&cast));
}

#[test]
fn an_ambush_needs_an_attack_on_you() {
    let mut t = Table::default();
    let ambush = t.card(
        "{G}",
        "Instant",
        None,
        "Cast this spell only during the declare attackers step and only if you've been \
         attacked this step.\nYou gain 3 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let a = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(ambush, P1, Zone::Hand);
    g.main();
    let ok = |g: &Game| mtg_engine::cost::cast_conditions_met(&g.engine.state, &g.table, s, P1);
    g.engine.state.step = Step::DeclareAttackers;
    assert!(!ok(&g), "no attack yet");
    g.engine
        .state
        .combat
        .attackers
        .insert(a, Target::Player(P1));
    assert!(ok(&g), "attacked during declare attackers");
    g.engine.state.step = Step::DeclareBlockers;
    assert!(!ok(&g), "but only in that step");
}

#[test]
fn an_attack_trigger_targets_what_the_defending_player_controls() {
    let mut t = Table::default();
    let raider = t.card(
        "{2}{W}",
        "Creature — Soldier",
        Some((2, 2)),
        "Whenever this creature attacks, tap target creature defending player controls.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let a = g.put(raider, P0, Zone::Battlefield);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    g.main();
    let face = mtg_ir::PrintedCards::face(&g.table, raider, 0)
        .unwrap()
        .clone();
    g.engine.state.combat.defending_player = Some(P1);
    let legal = mtg_engine::targeting::legal_targets(
        &g.engine.state,
        &g.table,
        &face.abilities[0].targets[0],
        a,
        P0,
        &[],
    );
    assert_eq!(legal, vec![Target::Object(theirs)], "not {mine:?}");
    g.engine.state.combat.defending_player = None;
    g.combat(&[a], &[], &[Target::Object(theirs)], &[]);
    assert!(g.engine.state.objects[&theirs].tapped);
}

#[test]
fn unblocked_attackers_trigger_and_poison() {
    for blocked in [false, true] {
        let mut t = Table::default();
        let snake = t.card(
            "{1}{G}",
            "Creature — Elf",
            Some((1, 1)),
            "Whenever this creature attacks and isn't blocked, defending player gets a poison \
             counter.",
        );
        let wall = t.card("{1}{W}", "Creature — Wall", Some((0, 4)), "Defender");
        let mut g = Game::new(t);
        let s = g.put(snake, P0, Zone::Battlefield);
        let w = g.put(wall, P1, Zone::Battlefield);
        g.main();
        let blocks = if blocked { vec![(w, s)] } else { vec![] };
        g.combat(&[s], &blocks, &[], &[]);
        assert_eq!(g.engine.state.player(P1).poison, u32::from(!blocked));
        assert_eq!(g.life(P1), if blocked { 20 } else { 19 });
    }
}

#[test]
fn unblocked_also_when_nothing_could_block() {
    let mut t = Table::default();
    let snake = t.card(
        "{1}{G}",
        "Creature — Elf",
        Some((1, 1)),
        "Whenever this creature attacks and isn't blocked, defending player loses 2 life.",
    );
    let mut g = Game::new(t);
    let s = g.put(snake, P0, Zone::Battlefield);
    g.main();
    g.combat(&[s], &[], &[], &[]);
    assert_eq!(g.life(P1), 17, "2 from the trigger, 1 from combat");
}

#[test]
fn blockable_only_by_flying_or_reach() {
    let mut t = Table::default();
    let kite = t.card(
        "{1}{U}",
        "Creature — Spirit",
        Some((1, 1)),
        "This creature can't be blocked except by creatures with flying or reach.",
    );
    let archer = t.card("{1}{G}", "Creature — Elf", Some((1, 2)), "Reach");
    let bear = t.bear();
    let mut g = Game::new(t);
    let k = g.put(kite, P0, Zone::Battlefield);
    let a = g.put(archer, P1, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.engine
        .state
        .combat
        .attackers
        .insert(k, Target::Player(P1));
    let can = |x| mtg_engine::combat::can_block(&g.engine.state, &g.table, x, k);
    assert!(can(a));
    assert!(!can(b));
}

#[test]
fn cant_block_unless_you_control_a_kind() {
    for with_friend in [false, true] {
        let mut t = Table::default();
        let coward = t.card(
            "{B}",
            "Creature — Zombie",
            Some((2, 2)),
            "This creature can't block unless you control a Goblin.",
        );
        let goblin = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
        let bear = t.bear();
        let mut g = Game::new(t);
        let a = g.put(bear, P0, Zone::Battlefield);
        let c = g.put(coward, P1, Zone::Battlefield);
        if with_friend {
            g.put(goblin, P1, Zone::Battlefield);
        }
        g.main();
        g.engine
            .state
            .combat
            .attackers
            .insert(a, Target::Player(P1));
        let eligible = mtg_engine::combat::eligible_blockers(&g.engine.state, &g.table, P1);
        assert_eq!(eligible.iter().any(|(b, _)| *b == c), with_friend);
    }
}

#[test]
fn cant_block_big_creatures_and_cant_be_blocked() {
    let mut t = Table::default();
    let picky = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "This creature can't block creatures with power 3 or greater.",
    );
    let ghost = t.card(
        "{1}{U}",
        "Creature — Spirit",
        Some((1, 1)),
        "This creature can't block and can't be blocked.",
    );
    let bear = t.bear();
    let ogre = t.card("{2}{R}", "Creature — Goblin", Some((3, 3)), "");
    let mut g = Game::new(t);
    let b = g.put(bear, P0, Zone::Battlefield);
    let o = g.put(ogre, P0, Zone::Battlefield);
    let gh = g.put(ghost, P0, Zone::Battlefield);
    let p = g.put(picky, P1, Zone::Battlefield);
    let gh2 = g.put(ghost, P1, Zone::Battlefield);
    g.main();
    for a in [b, o, gh] {
        g.engine
            .state
            .combat
            .attackers
            .insert(a, Target::Player(P1));
    }
    let can = |x, a| mtg_engine::combat::can_block(&g.engine.state, &g.table, x, a);
    assert!(can(p, b));
    assert!(!can(p, o), "power 3");
    assert!(!can(p, gh), "can't be blocked");
    let eligible = mtg_engine::combat::eligible_blockers(&g.engine.state, &g.table, P1);
    assert!(!eligible.iter().any(|(x, _)| *x == gh2), "can't block");
}
