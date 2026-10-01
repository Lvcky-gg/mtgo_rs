//! Combat: declaration, evasion, ordering, and damage (CR 506–511).

mod common;

use common::*;
use mtg_core::{ObjectId, Step, Zone, ZoneRef};
use mtg_engine::{
    Choice, Engine, Progress,
    choice::{Answer, ChoiceKind},
    state::GameState,
};

fn board() -> GameState {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.step = Step::BeginCombat;
    state.priority = Some(P0);
    state
}

/// A creature already on the battlefield and able to act.
fn ready(state: &mut GameState, card: mtg_core::CardId, who: mtg_core::PlayerId) -> ObjectId {
    let id = state.place(card, who, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    id
}

#[derive(Default)]
struct Plan {
    attackers: Vec<ObjectId>,
    /// (blocker, attacker) pairs.
    blocks: Vec<(ObjectId, ObjectId)>,
}

/// Play out combat with a fixed plan, returning every non-priority choice seen.
fn play_combat(engine: &mut Engine, cards: &TestCards, plan: &Plan) -> Vec<Choice> {
    let mut seen = Vec::new();

    for _ in 0..6000 {
        if engine.state.step == Step::PostcombatMain {
            return seen;
        }
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => return seen,
            Progress::NeedsChoice(c) => {
                let answer = match &c.kind {
                    ChoiceKind::Priority { .. } => Answer::Pass,
                    ChoiceKind::DeclareAttackers { .. } => {
                        seen.push(c.clone());
                        Answer::Objects(plan.attackers.clone())
                    }
                    ChoiceKind::DeclareBlockers { .. } => {
                        seen.push(c.clone());
                        Answer::Blocks(plan.blocks.clone())
                    }
                    other => {
                        seen.push(c.clone());
                        match other {
                            ChoiceKind::OrderBlockers { blockers, .. } => {
                                Answer::Order((0..blockers.len()).collect())
                            }
                            _ => c.default.clone().unwrap_or(Answer::Pass),
                        }
                    }
                };
                if engine.answer(cards, c.id, answer).is_err() {
                    // The declaration was rejected as illegal. Fall back to the
                    // choice's own default — which for a block declaration means
                    // declining to block, not `Pass` (the wrong answer *shape*
                    // would be rejected too, and the game would never move on).
                    let fallback = c.default.clone().unwrap_or(Answer::Pass);
                    engine
                        .answer(cards, c.id, fallback)
                        .expect("default is always legal");
                }
            }
        }
    }
    seen
}

fn tapped(engine: &Engine, id: ObjectId) -> bool {
    engine.state.objects.get(&id).is_some_and(|o| o.tapped)
}

fn alive(engine: &Engine, id: ObjectId) -> bool {
    engine.state.objects.contains_key(&id)
}

fn eligible_attackers(
    engine: &Engine,
    cards: &TestCards,
    who: mtg_core::PlayerId,
) -> Vec<ObjectId> {
    mtg_engine::combat::eligible_attackers(&engine.state, cards, who)
}

// ---- who may attack ----------------------------------------------------

#[test]
fn declaring_an_attacker_taps_it() {
    let mut state = board();
    let a = ready(&mut state, DUMMY, P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![a],
            ..Default::default()
        },
    );
    assert!(
        tapped(&engine, a),
        "attacking taps the creature (CR 508.1f)"
    );
}

#[test]
fn vigilance_means_attacking_does_not_tap() {
    let mut state = board();
    let a = ready(&mut state, VIGILANT, P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![a],
            ..Default::default()
        },
    );
    assert!(!tapped(&engine, a), "vigilance (CR 702.21b)");
    assert!(
        engine.state.combat.attackers.is_empty(),
        "combat is cleared by end of combat"
    );
}

#[test]
fn a_tapped_creature_cannot_attack() {
    let mut state = board();
    let a = ready(&mut state, DUMMY, P0);
    state.objects.get_mut(&a).unwrap().tapped = true;
    let engine = Engine::new(state);
    let cards = TestCards::default();

    assert!(!eligible_attackers(&engine, &cards, P0).contains(&a));
}

#[test]
fn a_creature_that_just_arrived_cannot_attack_without_haste() {
    let mut state = board();
    let a = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&a).unwrap().summoning_sick = true;
    let engine = Engine::new(state);
    let cards = TestCards::default();

    assert!(
        !eligible_attackers(&engine, &cards, P0).contains(&a),
        "summoning sickness (CR 302.6)"
    );
}

#[test]
fn defender_cannot_attack() {
    let mut state = board();
    let w = ready(&mut state, WALL, P0);
    let engine = Engine::new(state);
    let cards = TestCards::default();

    assert!(
        !eligible_attackers(&engine, &cards, P0).contains(&w),
        "CR 702.3b"
    );
}

#[test]
fn declining_to_attack_is_legal_and_needs_no_prompt_answer() {
    let mut state = board();
    ready(&mut state, DUMMY, P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let seen = play_combat(&mut engine, &cards, &Plan::default());
    let declare = seen
        .iter()
        .find(|c| matches!(c.kind, ChoiceKind::DeclareAttackers { .. }))
        .expect("should be asked");
    assert!(
        matches!(&declare.default, Some(Answer::Objects(v)) if v.is_empty()),
        "not attacking is always legal, so there is a safe default"
    );
}

// ---- evasion -----------------------------------------------------------

#[test]
fn a_flyer_cannot_be_blocked_by_a_ground_creature() {
    let mut state = board();
    let flyer = ready(&mut state, FLYER, P0);
    let ground = ready(&mut state, DUMMY, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![flyer],
            blocks: vec![(ground, flyer)],
        },
    );

    // The block is illegal, so the flyer got through.
    assert_eq!(
        engine.state.player(P1).life,
        18,
        "2 damage through an illegal block"
    );
    assert!(alive(&engine, ground), "the ground creature never fought");
}

#[test]
fn reach_can_block_a_flyer() {
    let mut state = board();
    let flyer = ready(&mut state, FLYER, P0);
    let spider = ready(&mut state, REACHER, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![flyer],
            blocks: vec![(spider, flyer)],
        },
    );

    assert_eq!(engine.state.player(P1).life, 20, "the block held");
    // 2/2 flyer into a 1/3 reach blocker: both survive.
    assert!(alive(&engine, flyer) && alive(&engine, spider));
}

#[test]
fn menace_cannot_be_blocked_by_one_creature() {
    let mut state = board();
    let menacer = ready(&mut state, MENACER, P0);
    let one = ready(&mut state, DUMMY, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![menacer],
            blocks: vec![(one, menacer)],
        },
    );

    assert_eq!(
        engine.state.player(P1).life,
        18,
        "a single blocker is illegal, so it got through"
    );
}

#[test]
fn menace_can_be_blocked_by_two_creatures() {
    let mut state = board();
    let menacer = ready(&mut state, MENACER, P0);
    let a = ready(&mut state, DUMMY, P1);
    let b = ready(&mut state, DUMMY, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![menacer],
            blocks: vec![(a, menacer), (b, menacer)],
        },
    );

    assert_eq!(
        engine.state.player(P1).life,
        20,
        "two blockers is a legal block"
    );
    assert!(!alive(&engine, menacer), "a 2/2 into two 2/2 blockers dies");
}

#[test]
fn validate_rejects_one_creature_blocking_two_attackers() {
    use std::collections::BTreeMap;
    let mut state = board();
    let a1 = ready(&mut state, DUMMY, P0);
    let a2 = ready(&mut state, DUMMY, P0);
    let b = ready(&mut state, DUMMY, P1);
    state
        .combat
        .attackers
        .insert(a1, mtg_core::Target::Player(P1));
    state
        .combat
        .attackers
        .insert(a2, mtg_core::Target::Player(P1));

    let cards = TestCards::default();
    let mut blocks = BTreeMap::new();
    blocks.insert(a1, vec![b]);
    blocks.insert(a2, vec![b]);

    assert!(
        mtg_engine::combat::validate_blocks(&state, &cards, P1, &blocks).is_err(),
        "one creature cannot block two attackers"
    );
}

// ---- damage ------------------------------------------------------------

#[test]
fn an_unblocked_attacker_damages_the_defending_player() {
    let mut state = board();
    let a = ready(&mut state, DUMMY, P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![a],
            ..Default::default()
        },
    );
    assert_eq!(engine.state.player(P1).life, 18);
}

#[test]
fn a_blocked_attacker_and_its_blocker_damage_each_other() {
    let mut state = board();
    let a = ready(&mut state, DUMMY, P0);
    let b = ready(&mut state, DUMMY, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![a],
            blocks: vec![(b, a)],
        },
    );

    assert_eq!(engine.state.player(P1).life, 20, "no damage got through");
    assert!(!alive(&engine, a), "two 2/2s trade");
    assert!(!alive(&engine, b));
}

#[test]
fn first_strike_kills_before_the_blocker_can_hit_back() {
    // CR 510.4: a first striker deals damage in its own step. A 1/1 first striker
    // cannot kill a 2/2, so this checks the reverse: the 2/2 attacker with first
    // strike kills a 1/1 blocker before taking any damage.
    let mut state = board();
    let attacker = ready(&mut state, DUMMY, P0);
    let blocker = ready(&mut state, FIRST_STRIKER, P1); // 1/1 first strike
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![attacker],
            blocks: vec![(blocker, attacker)],
        },
    );

    // The 1/1 first striker deals 1 first, then the surviving 2/2 kills it.
    assert!(alive(&engine, attacker), "a 2/2 survives 1 damage");
    assert!(!alive(&engine, blocker), "the 1/1 dies to the 2/2's damage");
    // Still in the combat phase's aftermath, before cleanup: the first striker's
    // damage is marked on the survivor. Cleanup is covered separately in `turn.rs`.
    assert_eq!(
        engine.state.objects[&attacker].damage, 1,
        "the first striker's 1 damage is marked on the surviving attacker"
    );
}

#[test]
fn deathtouch_makes_a_single_point_of_damage_lethal() {
    let mut state = board();
    let big = ready(&mut state, BIG, P0); // 6/6
    let adder = ready(&mut state, DEATHTOUCHER, P1); // 1/1 deathtouch
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![big],
            blocks: vec![(adder, big)],
        },
    );

    assert!(
        !alive(&engine, big),
        "1 deathtouch damage kills a 6/6 (CR 702.2b)"
    );
    assert!(!alive(&engine, adder), "and the 6/6 kills it back");
}

#[test]
fn trample_pushes_excess_damage_through_to_the_player() {
    let mut state = board();
    let giant = ready(&mut state, TRAMPLER, P0); // 4/4 trample
    let chump = ready(&mut state, FIRST_STRIKER, P1); // 1/1
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![giant],
            blocks: vec![(chump, giant)],
        },
    );

    assert!(!alive(&engine, chump));
    assert_eq!(
        engine.state.player(P1).life,
        17,
        "1 damage was lethal for the blocker, so 3 trampled over (CR 702.19b)"
    );
}

#[test]
fn custom_trample_damage_sends_only_valid_excess_to_the_defender() {
    let mut state = board();
    let attacker = ready(&mut state, TRAMPLER, P0);
    let blockers: Vec<_> = (0..2)
        .map(|_| ready(&mut state, FIRST_STRIKER, P1))
        .collect();
    let cards = TestCards::default();
    let mut engine = Engine::new(state);
    let mut checked = false;
    for _ in 0..6000 {
        if engine.state.step == Step::PostcombatMain {
            break;
        }
        let Progress::NeedsChoice(choice) = engine.advance(&cards) else {
            continue;
        };
        let answer = match &choice.kind {
            ChoiceKind::Priority { .. } => Answer::Pass,
            ChoiceKind::DeclareAttackers { .. } => Answer::Objects(vec![attacker]),
            ChoiceKind::DeclareBlockers { .. } => {
                Answer::Blocks(blockers.iter().map(|b| (*b, attacker)).collect())
            }
            ChoiceKind::OrderBlockers { .. } => Answer::Order(vec![0, 1]),
            ChoiceKind::AssignCombatDamage { among, .. } => {
                assert!(
                    engine
                        .answer(
                            &cards,
                            choice.id,
                            Answer::DamageAssignment(vec![(among[0], 1)])
                        )
                        .is_err()
                );
                let Progress::NeedsChoice(retry) = engine.advance(&cards) else {
                    panic!("question must survive rejection");
                };
                assert_eq!(retry.id, choice.id);
                let rows = vec![(among[0], 2), (among[1], 1)];
                assert!(!matches!(&choice.default,
                    Some(Answer::DamageAssignment(default)) if *default == rows));
                checked = true;
                Answer::DamageAssignment(rows)
            }
            _ => choice.default.clone().unwrap_or(Answer::Pass),
        };
        engine.answer(&cards, choice.id, answer).unwrap();
    }
    assert!(checked);
    assert_eq!(engine.state.step, Step::PostcombatMain);
    assert!(blockers.iter().all(|blocker| !alive(&engine, *blocker)));
    assert_eq!(engine.state.player(P1).life, 19);
}

#[test]
fn forced_damage_detection_handles_large_combined_lethal_thresholds() {
    let mut state = board();
    let attacker = ready(&mut state, BIG, P0);
    let blockers: Vec<_> = (0..3)
        .map(|_| {
            let blocker = ready(&mut state, BIG, P1);
            state
                .objects
                .get_mut(&blocker)
                .unwrap()
                .counters
                .insert(mtg_core::CounterKind::PlusOnePlusOne, i32::MAX - 6);
            blocker
        })
        .collect();
    state.combat.blocks.insert(attacker, blockers);
    assert!(mtg_engine::combat::assignment_is_forced(
        &state,
        &TestCards::default(),
        attacker
    ));
}

#[test]
fn custom_damage_uses_deathtouch_lethal_thresholds() {
    let mut state = board();
    let attacker = ready(&mut state, DEATHTOUCHER, P0);
    state
        .objects
        .get_mut(&attacker)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::PlusOnePlusOne, 3);
    let blockers: Vec<_> = (0..3).map(|_| ready(&mut state, BIG, P1)).collect();
    let cards = TestCards::default();
    let mut engine = Engine::new(state);
    let mut checked = false;
    for _ in 0..6000 {
        if engine.state.step == Step::PostcombatMain {
            break;
        }
        let Progress::NeedsChoice(choice) = engine.advance(&cards) else {
            continue;
        };
        let answer = match &choice.kind {
            ChoiceKind::Priority { .. } => Answer::Pass,
            ChoiceKind::DeclareAttackers { .. } => Answer::Objects(vec![attacker]),
            ChoiceKind::DeclareBlockers { .. } => {
                Answer::Blocks(blockers.iter().map(|b| (*b, attacker)).collect())
            }
            ChoiceKind::OrderBlockers { .. } => Answer::Order(vec![0, 1, 2]),
            ChoiceKind::AssignCombatDamage { among, total, .. } => {
                assert_eq!(*total, 4);
                let rows = vec![(among[0], 1), (among[1], 2), (among[2], 1)];
                assert!(!matches!(&choice.default,
                    Some(Answer::DamageAssignment(default)) if *default == rows));
                checked = true;
                Answer::DamageAssignment(rows)
            }
            _ => choice.default.clone().unwrap_or(Answer::Pass),
        };
        engine.answer(&cards, choice.id, answer).unwrap();
    }
    assert!(checked);
    assert_eq!(engine.state.step, Step::PostcombatMain);
    assert!(blockers.iter().all(|blocker| !alive(&engine, *blocker)));
}

#[test]
fn lethal_damage_does_not_wrap_large_marked_damage() {
    let mut state = board();
    let victim = ready(&mut state, DUMMY, P1);
    let cards = TestCards::default();
    for marked in [0, 1, 2, i32::MAX as u32 + 1, u32::MAX] {
        state.objects.get_mut(&victim).unwrap().damage = marked;
        assert_eq!(
            mtg_engine::combat::lethal_damage(&state, &cards, victim, false),
            2_u32.saturating_sub(marked)
        );
    }
}

#[test]
fn custom_damage_rejects_invalid_rows_without_consuming_the_question() {
    let mut state = board();
    let attacker = ready(&mut state, BIG, P0);
    let first = ready(&mut state, DUMMY, P1);
    let second = ready(&mut state, DUMMY, P1);
    let cards = TestCards::default();
    let mut engine = Engine::new(state);
    let mut checked = false;
    for _ in 0..6000 {
        if engine.state.step == Step::PostcombatMain {
            break;
        }
        let Progress::NeedsChoice(choice) = engine.advance(&cards) else {
            continue;
        };
        let answer = match &choice.kind {
            ChoiceKind::Priority { .. } => Answer::Pass,
            ChoiceKind::DeclareAttackers { .. } => Answer::Objects(vec![attacker]),
            ChoiceKind::DeclareBlockers { .. } => {
                Answer::Blocks(vec![(first, attacker), (second, attacker)])
            }
            ChoiceKind::OrderBlockers { .. } => Answer::Order(vec![0, 1]),
            ChoiceKind::AssignCombatDamage { among, .. } => {
                let first = among[0];
                let second = among[1];
                for rows in [
                    vec![(first, 3), (first, 3)],
                    vec![(attacker, 6)],
                    vec![(first, u32::MAX), (second, u32::MAX)],
                    vec![(first, 5)],
                    vec![(second, 6)],
                ] {
                    assert!(
                        engine
                            .answer(&cards, choice.id, Answer::DamageAssignment(rows))
                            .is_err()
                    );
                    let Progress::NeedsChoice(retry) = engine.advance(&cards) else {
                        panic!("question must survive rejection");
                    };
                    assert_eq!(retry.id, choice.id);
                }
                checked = true;
                Answer::DamageAssignment(vec![(first, 6)])
            }
            _ => choice.default.clone().unwrap_or(Answer::Pass),
        };
        engine.answer(&cards, choice.id, answer).unwrap();
    }
    assert!(checked);
    assert_eq!(engine.state.step, Step::PostcombatMain);
    assert_eq!(
        usize::from(alive(&engine, first)) + usize::from(alive(&engine, second)),
        1
    );
}

#[test]
fn accepting_default_damage_preserves_trample_past_multiple_blockers() {
    let mut state = board();
    let giant = ready(&mut state, TRAMPLER, P0);
    let first = ready(&mut state, FIRST_STRIKER, P1);
    let second = ready(&mut state, FIRST_STRIKER, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    let choices = play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![giant],
            blocks: vec![(first, giant), (second, giant)],
        },
    );
    assert!(
        choices
            .iter()
            .any(|c| matches!(c.kind, ChoiceKind::AssignCombatDamage { .. }))
    );
    assert!(!alive(&engine, first));
    assert!(!alive(&engine, second));
    assert_eq!(
        engine.state.player(P1).life,
        18,
        "accepting the default must retain two excess trample damage"
    );
}

#[test]
fn without_trample_all_damage_stops_at_the_blocker() {
    let mut state = board();
    let big = ready(&mut state, BIG, P0); // 6/6, no trample
    let chump = ready(&mut state, FIRST_STRIKER, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![big],
            blocks: vec![(chump, big)],
        },
    );

    assert_eq!(
        engine.state.player(P1).life,
        20,
        "no trample means nothing gets through"
    );
}

// ---- prompts only when they matter -------------------------------------

#[test]
fn a_single_blocker_needs_no_ordering_prompt() {
    let mut state = board();
    let a = ready(&mut state, BIG, P0);
    let b = ready(&mut state, DUMMY, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let seen = play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![a],
            blocks: vec![(b, a)],
        },
    );
    assert!(
        !seen
            .iter()
            .any(|c| matches!(c.kind, ChoiceKind::OrderBlockers { .. })),
        "nothing to order with one blocker"
    );
}

#[test]
fn two_blockers_do_need_an_ordering_prompt() {
    // CR 509.2 — the attacking player orders them, because damage assignment
    // depends on it.
    let mut state = board();
    let a = ready(&mut state, BIG, P0);
    let b1 = ready(&mut state, DUMMY, P1);
    let b2 = ready(&mut state, DUMMY, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let seen = play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![a],
            blocks: vec![(b1, a), (b2, a)],
        },
    );

    let prompt = seen
        .iter()
        .find(|c| matches!(c.kind, ChoiceKind::OrderBlockers { .. }))
        .expect("should be asked to order two blockers");
    assert_eq!(prompt.who, P0, "the attacking player orders them");
}

#[test]
fn damage_assignment_is_not_asked_when_it_is_forced() {
    // A 4/4 into two 2/2 blockers has exactly enough for both: no spare damage, so
    // no decision. Same principle as the trigger ordering analysis.
    let mut state = board();
    let a = ready(&mut state, TRAMPLER, P0); // 4/4
    let b1 = ready(&mut state, DUMMY, P1); // 2/2
    let b2 = ready(&mut state, DUMMY, P1); // 2/2
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let seen = play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![a],
            blocks: vec![(b1, a), (b2, a)],
        },
    );

    assert!(
        !seen
            .iter()
            .any(|c| matches!(c.kind, ChoiceKind::AssignCombatDamage { .. })),
        "4 power against two 2-toughness blockers is forced, got {seen:#?}"
    );
}

#[test]
fn damage_assignment_is_asked_when_there_is_spare_damage() {
    let mut state = board();
    let a = ready(&mut state, BIG, P0); // 6/6 against 4 total toughness
    let b1 = ready(&mut state, DUMMY, P1);
    let b2 = ready(&mut state, DUMMY, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let seen = play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![a],
            blocks: vec![(b1, a), (b2, a)],
        },
    );

    assert!(
        seen.iter()
            .any(|c| matches!(c.kind, ChoiceKind::AssignCombatDamage { .. })),
        "6 power against 4 toughness leaves a real choice"
    );
}

// ---- cleanup -----------------------------------------------------------

#[test]
fn end_of_combat_clears_the_combat_state() {
    let mut state = board();
    let a = ready(&mut state, DUMMY, P0);
    let b = ready(&mut state, SENTRY, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![a],
            blocks: vec![(b, a)],
        },
    );

    assert!(engine.state.combat.attackers.is_empty());
    assert!(engine.state.combat.blocks.is_empty());
    assert!(engine.state.combat.was_blocked.is_empty());
}

#[test]
fn a_creature_that_dies_in_combat_reaches_the_graveyard() {
    let mut state = board();
    let a = ready(&mut state, DUMMY, P0);
    let b = ready(&mut state, DUMMY, P1);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play_combat(
        &mut engine,
        &cards,
        &Plan {
            attackers: vec![a],
            blocks: vec![(b, a)],
        },
    );

    assert_eq!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .len(),
        1
    );
    assert_eq!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P1))
            .len(),
        1
    );
}

// ---- lifelink (CR 702.15) ---------------------------------------------------

#[test]
fn lifelink_combat_damage_gains_its_controller_that_much_life() {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.step = Step::BeginCombat;
    state.priority = Some(P0);
    for p in [P0, P1] {
        for _ in 0..10 {
            state.place(DUMMY, p, ZoneRef::of(Zone::Library, p));
        }
    }
    let knight = state.place(LIFELINKER, P0, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&knight).unwrap().summoning_sick = false;
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    for _ in 0..3000 {
        if engine.state.step == Step::PostcombatMain {
            break;
        }
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
                let answer = match &c.kind {
                    ChoiceKind::DeclareAttackers { eligible, .. } => {
                        Answer::Objects(eligible.clone())
                    }
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                engine.answer(&cards, c.id, answer).unwrap();
            }
        }
    }
    assert_eq!(engine.state.player(P1).life, 17, "3 damage dealt");
    assert_eq!(engine.state.player(P0).life, 23, "and 3 life gained");
}
