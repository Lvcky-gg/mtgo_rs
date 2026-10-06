//! Block requirements (CR 509.1c): lures, "must be blocked if able", "blocks each combat
//! if able", and the one-shot "this turn" versions.
use std::collections::BTreeMap;

use super::harness::*;
use mtg_core::{ObjectId, Target, Zone};
use mtg_engine::{
    Progress,
    choice::{Answer, Choice, ChoiceKind},
    combat::BlockError,
};

fn attacking(g: &mut Game, attackers: &[ObjectId]) {
    for a in attackers {
        g.engine
            .state
            .combat
            .attackers
            .insert(*a, Target::Player(P1));
    }
}

fn blocks(g: &Game, pairs: &[(ObjectId, Vec<ObjectId>)]) -> Result<(), BlockError> {
    let blocks: BTreeMap<_, _> = pairs.iter().cloned().collect();
    mtg_engine::combat::validate_blocks(&g.engine.state, &g.table, P1, &blocks)
}

/// From P0's main phase, attack with `attackers` and stop at P1's block declaration.
fn to_blockers(g: &mut Game, attackers: &[ObjectId]) -> Choice {
    let c = g.pending.take().expect("no priority prompt");
    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
    loop {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::DeclareAttackers { .. } => {
                    g.engine
                        .answer(&g.table, c.id, Answer::Objects(attackers.to_vec()))
                        .unwrap();
                }
                ChoiceKind::DeclareBlockers { .. } => return c,
                _ => {
                    let a = c.default.clone().unwrap_or(Answer::Pass);
                    g.engine.answer(&g.table, c.id, a).unwrap();
                }
            },
            Progress::GameOver { .. } => panic!("game ended"),
        }
    }
}

#[test]
fn every_creature_able_to_block_a_lure_does() {
    let mut t = Table::default();
    let lure = t.card(
        "{2}{G}",
        "Creature — Beast",
        Some((2, 2)),
        "All creatures able to block this creature do so.",
    );
    let bear = t.bear();
    let flier = t.card("{1}{W}", "Creature — Bird", Some((1, 1)), "Flying");
    let mut g = Game::new(t);
    let l = g.put(lure, P0, Zone::Battlefield);
    let other = g.put(flier, P0, Zone::Battlefield);
    let b1 = g.put(bear, P1, Zone::Battlefield);
    let b2 = g.put(bear, P1, Zone::Battlefield);
    g.main();
    attacking(&mut g, &[l, other]);
    assert!(blocks(&g, &[]).is_err(), "nobody blocked the lure");
    assert!(
        blocks(&g, &[(l, vec![b1])]).is_err(),
        "the second bear could too"
    );
    assert!(blocks(&g, &[(l, vec![b1, b2])]).is_ok());
    assert_eq!(
        mtg_engine::combat::required_blocks(&g.engine.state, &g.table, P1),
        vec![(b1, l), (b2, l)],
        "the default obeys the requirement"
    );
}

#[test]
fn a_lure_only_pulls_creatures_that_could_block_it() {
    let mut t = Table::default();
    let lure = t.card(
        "{2}{U}",
        "Creature — Bird",
        Some((2, 2)),
        "Flying\nAll creatures able to block this creature do so.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let l = g.put(lure, P0, Zone::Battlefield);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.main();
    attacking(&mut g, &[l, attacker]);
    assert!(blocks(&g, &[]).is_ok(), "the bear can't block a flier");
    assert!(blocks(&g, &[(attacker, vec![b])]).is_ok());
}

#[test]
fn must_be_blocked_needs_one_blocker_and_respects_menace() {
    for (menace, defenders) in [(false, 1), (true, 1), (true, 2)] {
        let mut t = Table::default();
        let text = if menace {
            "Menace\nThis creature must be blocked if able."
        } else {
            "This creature must be blocked if able."
        };
        let bait = t.card("{1}{R}", "Creature — Goblin", Some((2, 2)), text);
        let bear = t.bear();
        let mut g = Game::new(t);
        let a = g.put(bait, P0, Zone::Battlefield);
        let other = g.put(bear, P0, Zone::Battlefield);
        let bs: Vec<_> = (0..defenders)
            .map(|_| g.put(bear, P1, Zone::Battlefield))
            .collect();
        g.main();
        attacking(&mut g, &[a, other]);
        let able = !menace || defenders >= 2;
        assert_eq!(blocks(&g, &[]).is_ok(), !able, "{menace} {defenders}");
        assert_eq!(
            blocks(&g, &[(other, vec![bs[0]])]).is_ok(),
            !able,
            "blocking the other attacker instead"
        );
        if able {
            assert!(blocks(&g, &[(a, bs.clone())]).is_ok());
        }
    }
}

#[test]
fn a_creature_that_blocks_each_combat_if_able() {
    for flier in [false, true] {
        let mut t = Table::default();
        let guard = t.card(
            "{1}{R}",
            "Creature — Goblin",
            Some((1, 1)),
            "This creature blocks each combat if able.",
        );
        let attacker = t.card(
            "{1}{W}",
            "Creature — Bird",
            Some((2, 2)),
            if flier { "Flying" } else { "" },
        );
        let mut g = Game::new(t);
        let a = g.put(attacker, P0, Zone::Battlefield);
        let gd = g.put(guard, P1, Zone::Battlefield);
        g.main();
        attacking(&mut g, &[a]);
        assert_eq!(blocks(&g, &[]).is_ok(), flier, "it must block if it can");
        if !flier {
            assert!(blocks(&g, &[(a, vec![gd])]).is_ok());
        }
    }
}

#[test]
fn the_engine_rejects_a_declaration_that_ignores_a_lure_and_defaults_to_one_that_obeys() {
    let mut t = Table::default();
    let lure = t.card(
        "{2}{G}",
        "Creature — Beast",
        Some((5, 5)),
        "All creatures able to block this creature do so.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let l = g.put(lure, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.main();
    let c = to_blockers(&mut g, &[l]);
    assert!(
        matches!(&c.default, Some(Answer::Blocks(p)) if *p == vec![(b, l)]),
        "{:?}",
        c.default
    );
    assert!(
        g.engine
            .answer(&g.table, c.id, Answer::Blocks(Vec::new()))
            .is_err(),
        "no blocks is illegal"
    );
    let c = loop {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) => break c,
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended"),
        }
    };
    assert!(
        matches!(c.kind, ChoiceKind::DeclareBlockers { .. }),
        "re-asked"
    );
    let default = c.default.clone().unwrap();
    g.engine.answer(&g.table, c.id, default).unwrap();
    assert_eq!(g.engine.state.combat.blocks.get(&l), Some(&vec![b]));
}

#[test]
fn target_creature_blocks_this_turn_if_able() {
    let mut t = Table::default();
    let taunt = t.card(
        "{R}",
        "Sorcery",
        None,
        "Target creature blocks this turn if able.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(taunt, P0, Zone::Hand);
    let a = g.put(bear, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    let free = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.cast(s, &[Target::Object(b)]);
    attacking(&mut g, &[a]);
    assert!(blocks(&g, &[]).is_err());
    assert!(
        blocks(&g, &[(a, vec![free])]).is_err(),
        "the other one blocking"
    );
    assert!(blocks(&g, &[(a, vec![b])]).is_ok());
}

#[test]
fn target_creature_blocks_this_creature_if_able() {
    let mut t = Table::default();
    let taunter = t.card(
        "{1}{R}",
        "Creature — Goblin",
        Some((2, 2)),
        "{1}: Target creature blocks this creature this turn if able.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let tn = g.put(taunter, P0, Zone::Battlefield);
    let other = g.put(bear, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.act(activate(tn, 0), &[Target::Object(b)], &[]);
    attacking(&mut g, &[tn, other]);
    assert!(blocks(&g, &[]).is_err());
    assert!(
        blocks(&g, &[(other, vec![b])]).is_err(),
        "it blocks the taunter"
    );
    assert!(blocks(&g, &[(tn, vec![b])]).is_ok());
}

#[test]
fn all_creatures_able_to_block_target_creature_this_turn_do_so() {
    let mut t = Table::default();
    let spell = t.card(
        "{G}",
        "Instant",
        None,
        "Target creature gets +2/+2 until end of turn. All creatures able to block it this \
         turn do so.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(spell, P0, Zone::Hand);
    let a = g.put(bear, P0, Zone::Battlefield);
    let other = g.put(bear, P0, Zone::Battlefield);
    let b1 = g.put(bear, P1, Zone::Battlefield);
    let b2 = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.cast(s, &[Target::Object(a)]);
    assert_eq!(g.pt(a), (4, 4));
    attacking(&mut g, &[a, other]);
    assert!(blocks(&g, &[(a, vec![b1]), (other, vec![b2])]).is_err());
    assert!(blocks(&g, &[(a, vec![b1, b2])]).is_ok());
}

#[test]
fn a_pump_that_must_be_blocked_this_turn() {
    let mut t = Table::default();
    let spell = t.card(
        "{R}",
        "Instant",
        None,
        "Target creature gets +2/+2 until end of turn and must be blocked this turn if able.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(spell, P0, Zone::Hand);
    let a = g.put(bear, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.cast(s, &[Target::Object(a)]);
    assert_eq!(g.pt(a), (4, 4));
    attacking(&mut g, &[a]);
    assert!(blocks(&g, &[]).is_err());
    assert!(blocks(&g, &[(a, vec![b])]).is_ok());
}

// ---- assigning combat damage as though it weren't blocked ------------------------------------

#[test]
fn it_may_assign_its_damage_as_though_it_werent_blocked() {
    for unblocked in [false, true] {
        let mut t = Table::default();
        let elemental = t.card(
            "{5}{G}{G}",
            "Creature — Elemental",
            Some((5, 5)),
            "You may have this creature assign its combat damage as though it weren't blocked.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        let a = g.put(elemental, P0, Zone::Battlefield);
        let b = g.put(bear, P1, Zone::Battlefield);
        g.main();
        // Assigning nothing to the blockers is the "as though unblocked" choice; otherwise
        // the default assignment stands.
        let answers = if unblocked {
            vec![Answer::DamageAssignment(Vec::new())]
        } else {
            Vec::new()
        };
        g.combat(&[a], &[(b, a)], &[], &answers);
        assert_eq!(g.life(P1), if unblocked { 15 } else { 20 });
        assert_eq!(
            g.engine.state.objects.contains_key(&b),
            unblocked,
            "{unblocked}"
        );
    }
}

#[test]
fn an_ordinary_blocked_creature_cant_skip_its_blockers() {
    let mut t = Table::default();
    let bear = t.bear();
    let giant = t.card("{4}{R}", "Creature — Giant", Some((5, 5)), "");
    let mut g = Game::new(t);
    let a = g.put(giant, P0, Zone::Battlefield);
    let b1 = g.put(bear, P1, Zone::Battlefield);
    let b2 = g.put(bear, P1, Zone::Battlefield);
    g.main();
    let c = to_blockers(&mut g, &[a]);
    g.engine
        .answer(&g.table, c.id, Answer::Blocks(vec![(b1, a), (b2, a)]))
        .unwrap();
    let c = loop {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) if matches!(c.kind, ChoiceKind::AssignCombatDamage { .. }) => {
                break c;
            }
            Progress::NeedsChoice(c) => {
                let a = c.default.clone().unwrap_or(Answer::Pass);
                g.engine.answer(&g.table, c.id, a).unwrap();
            }
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended"),
        }
    };
    assert!(
        g.engine
            .answer(&g.table, c.id, Answer::DamageAssignment(Vec::new()))
            .is_err()
    );
}

#[test]
fn for_each_creature_you_control_that_matches_it_may_assign_as_though_unblocked() {
    let mut t = Table::default();
    let lord = t.card(
        "{3}{G}",
        "Creature — Goblin",
        Some((1, 1)),
        "For each non-Goblin creature you control, you may have that creature assign its \
         combat damage as though it weren't blocked.",
    );
    let giant = t.card("{4}{G}", "Creature — Giant", Some((4, 4)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(lord, P0, Zone::Battlefield);
    let a = g.put(giant, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.combat(
        &[a],
        &[(b, a)],
        &[],
        &[Answer::DamageAssignment(Vec::new())],
    );
    assert_eq!(g.life(P1), 16);
}

#[test]
fn an_activated_ability_lets_it_assign_as_though_unblocked_this_turn() {
    let mut t = Table::default();
    let rhino = t.card(
        "{3}{G}",
        "Creature — Rhino",
        Some((4, 4)),
        "{1}: You may have this creature assign its combat damage this turn as though it \
         weren't blocked.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let a = g.put(rhino, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.act(activate(a, 0), &[], &[]);
    g.combat(
        &[a],
        &[(b, a)],
        &[],
        &[Answer::DamageAssignment(Vec::new())],
    );
    assert_eq!(g.life(P1), 16);
}

// ---- provoke and "blocks it this combat" ------------------------------------------------------

/// From P0's main phase, attack with `attacker`, target `target` with its attack trigger and
/// say yes to "you may", and stop at P1's block declaration.
fn provoke_to_blockers(g: &mut Game, attacker: ObjectId, target: ObjectId) -> Choice {
    let c = g.pending.take().expect("no priority prompt");
    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
    loop {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => {
                let a = match &c.kind {
                    ChoiceKind::DeclareAttackers { .. } => Answer::Objects(vec![attacker]),
                    ChoiceKind::DeclareBlockers { .. } => return c,
                    ChoiceKind::ChooseTargets { .. } => {
                        Answer::Targets(vec![vec![Target::Object(target)]])
                    }
                    ChoiceKind::Confirm => Answer::Bool(true),
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                g.engine.answer(&g.table, c.id, a).unwrap();
            }
            Progress::GameOver { .. } => panic!("game ended"),
        }
    }
}

#[test]
fn provoke_untaps_the_target_and_makes_it_block_for_this_combat() {
    let mut t = Table::default();
    let provoker = t.card("{1}{G}", "Creature — Elf", Some((3, 3)), "Provoke");
    let bear = t.bear();
    let mut g = Game::new(t);
    let p = g.put(provoker, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    let other = g.put(bear, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&b).unwrap().tapped = true;
    g.main();
    let c = provoke_to_blockers(&mut g, p, b);
    assert!(!g.engine.state.objects[&b].tapped, "provoke untapped it");
    assert!(blocks(&g, &[]).is_err());
    assert!(
        blocks(&g, &[(p, vec![other])]).is_err(),
        "the provoked one must block"
    );
    assert!(
        matches!(&c.default, Some(Answer::Blocks(pairs)) if *pairs == vec![(b, p)]),
        "{:?}",
        c.default
    );
    g.engine
        .answer(&g.table, c.id, Answer::Blocks(vec![(b, p)]))
        .unwrap();
    g.until(P0, mtg_core::Step::End);
    assert!(
        !g.engine.state.continuous.iter().any(|e| matches!(
            e.modification,
            mtg_ir::effect::Modification::Restriction(mtg_ir::effect::Restriction::MustBlockSource)
        )),
        "the requirement ended with combat"
    );
}

#[test]
fn an_attack_trigger_makes_a_target_block_it() {
    let mut t = Table::default();
    let brute = t.card(
        "{2}{R}",
        "Creature — Goblin",
        Some((3, 3)),
        "Whenever this creature attacks, target creature defending player controls blocks it \
         this combat if able.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let a = g.put(brute, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    g.main();
    let c = provoke_to_blockers(&mut g, a, b);
    assert!(matches!(&c.default, Some(Answer::Blocks(pairs)) if *pairs == vec![(b, a)]));
    assert!(blocks(&g, &[]).is_err());
}

#[test]
fn unblockable_while_the_defending_player_controls_an_artifact() {
    for artifact in [false, true] {
        let mut t = Table::default();
        let thief = t.card(
            "{1}{U}",
            "Creature — Wizard",
            Some((2, 1)),
            "This creature can't be blocked as long as defending player controls an artifact.",
        );
        let relic = t.card("{1}", "Artifact", None, "");
        let bear = t.bear();
        let mut g = Game::new(t);
        let a = g.put(thief, P0, Zone::Battlefield);
        let b = g.put(bear, P1, Zone::Battlefield);
        if artifact {
            g.put(relic, P1, Zone::Battlefield);
        }
        g.main();
        // With the artifact nothing can block it, so the bear's block is never even asked
        // for.
        g.combat(&[a], &[(b, a)], &[], &[]);
        assert_eq!(g.life(P1), if artifact { 18 } else { 20 });
    }
}

#[test]
fn whenever_you_attack_with_two_or_more_creatures() {
    for attackers in [1usize, 2] {
        let mut t = Table::default();
        let banner = t.card(
            "{2}{W}",
            "Enchantment",
            None,
            "Whenever you attack with two or more creatures, draw a card.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.put(banner, P0, Zone::Battlefield);
        let bs: Vec<_> = (0..2).map(|_| g.put(bear, P0, Zone::Battlefield)).collect();
        g.main();
        let hand = g.count(Zone::Hand, P0);
        g.combat(&bs[..attackers], &[], &[], &[]);
        assert_eq!(g.count(Zone::Hand, P0), hand + usize::from(attackers == 2));
    }
}
