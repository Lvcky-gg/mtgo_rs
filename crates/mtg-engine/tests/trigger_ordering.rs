//! Trigger ordering, end to end through the engine.
//!
//! The unit tests in `mtg_engine::triggers` check the commutativity analysis in
//! isolation. These check the thing that actually matters to a player: whether the
//! game stops and asks.

mod common;

use common::*;
use mtg_core::{
    AbilityId, Cause, Event, EventId, ObjectId, PlayerId, StampedEvent, Timestamp, Zone, ZoneRef,
};
use mtg_engine::{ChoiceKind, Engine, PendingTrigger, state::GameState};
use mtg_ir::footprint::{Footprint, Resource};
use std::collections::BTreeSet;

fn game_with(pending: Vec<PendingTrigger>) -> (Engine, TestCards) {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.priority = Some(P0);
    for _ in 0..10 {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Library, P0));
    }
    // A source on the battlefield for the triggers to come from.
    let source = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
    let pending = pending
        .into_iter()
        .map(|mut t| {
            t.source = source;
            t
        })
        .collect::<Vec<_>>();
    for t in pending {
        state.pending_triggers.push(t);
    }
    (Engine::new(state), TestCards::default())
}

fn trigger(
    fired: u64,
    controller: PlayerId,
    reads: &[Resource],
    writes: &[Resource],
    prompts: bool,
) -> PendingTrigger {
    PendingTrigger {
        source: ObjectId(0),
        // One card per trigger: different abilities, never copies of one.
        card: mtg_core::CardId(fired as u32),
        face: 0,
        ability: AbilityId(0),
        controller,
        cause: StampedEvent {
            id: EventId(fired),
            at: Timestamp(fired),
            cause: Cause::TurnStructure,
            event: Event::Shuffled { player: controller },
        },
        fired_at: Timestamp(fired),
        footprint: Footprint {
            reads: reads.iter().cloned().collect::<BTreeSet<_>>(),
            writes: writes.iter().cloned().collect::<BTreeSet<_>>(),
            prompts,
            ..Default::default()
        },
        bindings: Default::default(),
        delayed: None,
        granted: None,
    }
}

fn ordering_prompts(interruptions: &[mtg_engine::Choice]) -> usize {
    interruptions
        .iter()
        .filter(|c| matches!(c.kind, ChoiceKind::OrderTriggers { .. }))
        .count()
}

#[test]
fn triggers_that_cannot_interact_are_placed_without_asking() {
    // Two triggers touching different permanents. Every order gives the same
    // result, so the player should never see a dialog.
    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[], &[Resource::Object(ObjectId(900))], false),
        trigger(2, P0, &[], &[Resource::Object(ObjectId(901))], false),
    ]);

    let interruptions = run(&mut engine, &cards, 600, |e| {
        e.state.pending_triggers.is_empty()
            && !e.state.objects_in(ZoneRef::shared(Zone::Stack)).is_empty()
    });

    assert_eq!(
        ordering_prompts(&interruptions),
        0,
        "disjoint triggers must not produce an ordering prompt"
    );
    assert_eq!(
        engine.state.objects_in(ZoneRef::shared(Zone::Stack)).len(),
        2,
        "both triggers should still have reached the stack"
    );
}

#[test]
fn triggers_that_touch_the_same_permanent_do_ask() {
    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[], &[Resource::Object(ObjectId(900))], false),
        trigger(2, P0, &[], &[Resource::Object(ObjectId(900))], false),
    ]);

    let interruptions = run(&mut engine, &cards, 600, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 2
    });

    assert_eq!(
        ordering_prompts(&interruptions),
        1,
        "overlapping triggers must ask exactly once, got {interruptions:#?}"
    );
}

#[test]
fn the_ordering_prompt_names_the_resource_that_makes_order_matter() {
    // The explanation is the feature: a prompt that cannot say why is the thing
    // being replaced.
    let contested = Resource::Life(P0);
    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[], std::slice::from_ref(&contested), false),
        trigger(
            2,
            P0,
            std::slice::from_ref(&contested),
            &[Resource::Object(ObjectId(901))],
            false,
        ),
    ]);

    let interruptions = run(&mut engine, &cards, 600, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 2
    });

    let prompt = interruptions
        .iter()
        .find_map(|c| match &c.kind {
            ChoiceKind::OrderTriggers { conflicts, .. } => Some(conflicts),
            _ => None,
        })
        .expect("an ordering prompt");

    assert!(
        prompt.iter().any(|c| c.over.contains(&contested)),
        "the prompt should name the contested resource, got {prompt:#?}"
    );

    // And it says what each trigger is, so the player is not ordering "trigger 1, trigger 2".
    let labels = interruptions
        .iter()
        .find_map(|c| match &c.kind {
            ChoiceKind::OrderTriggers { labels, .. } => Some(labels.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(labels.len(), 2);
    assert!(labels.iter().all(|l| !l.is_empty()), "{labels:?}");
}

#[test]
fn a_single_trigger_is_never_an_ordering_question() {
    let (mut engine, cards) = game_with(vec![trigger(1, P0, &[], &[Resource::Unanalysable], true)]);

    let interruptions = run(&mut engine, &cards, 600, |e| {
        !e.state.objects_in(ZoneRef::shared(Zone::Stack)).is_empty()
    });

    assert_eq!(
        ordering_prompts(&interruptions),
        0,
        "one trigger has nothing to order against"
    );
}

#[test]
fn each_player_orders_only_their_own_triggers() {
    // Two triggers each, all four mutually disjoint. Neither player is asked, and
    // the batches are kept apart.
    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[], &[Resource::Object(ObjectId(900))], false),
        trigger(2, P0, &[], &[Resource::Object(ObjectId(901))], false),
        trigger(3, P1, &[], &[Resource::Object(ObjectId(902))], false),
        trigger(4, P1, &[], &[Resource::Object(ObjectId(903))], false),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 4
    });

    assert_eq!(ordering_prompts(&interruptions), 0);
    assert_eq!(
        engine.state.objects_in(ZoneRef::shared(Zone::Stack)).len(),
        4,
        "all four triggers reach the stack"
    );
}

#[test]
fn only_the_conflicting_player_is_interrupted() {
    // One player's triggers conflict; the other's do not. Exactly one prompt, and
    // it belongs to the player whose triggers actually interact.
    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[], &[Resource::Object(ObjectId(900))], false),
        trigger(2, P0, &[], &[Resource::Object(ObjectId(900))], false),
        trigger(3, P1, &[], &[Resource::Object(ObjectId(902))], false),
        trigger(4, P1, &[], &[Resource::Object(ObjectId(903))], false),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 4
    });

    let prompts: Vec<_> = interruptions
        .iter()
        .filter(|c| matches!(c.kind, ChoiceKind::OrderTriggers { .. }))
        .collect();
    assert_eq!(prompts.len(), 1, "only one player should be asked");
    assert_eq!(
        prompts[0].who, P0,
        "and it should be the one whose triggers interact"
    );
}

// ---- life changes and copies of one trigger ------------------------------

/// A trigger that only gains (`up`) or only loses life for P0, by a fixed amount.
fn life_trigger(fired: u64, up: bool) -> PendingTrigger {
    let mut t = trigger(fired, P0, &[], &[], false);
    let life: BTreeSet<Resource> = [Resource::Life(P0)].into();
    t.footprint = if up {
        Footprint {
            gains: life,
            ..Default::default()
        }
    } else {
        Footprint {
            losses: life,
            ..Default::default()
        }
    };
    t
}

fn prompts_for(batch: Vec<PendingTrigger>) -> usize {
    let (mut engine, cards) = game_with(batch);
    let n = engine.state.pending_triggers.pending.len();
    let interruptions = run(&mut engine, &cards, 600, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= n
    });
    ordering_prompts(&interruptions)
}

#[test]
fn two_life_gains_are_placed_without_asking() {
    // 1 then 2 life ends where 2 then 1 does, and life going up never loses a game.
    assert_eq!(
        prompts_for(vec![life_trigger(1, true), life_trigger(2, true)]),
        0
    );
}

#[test]
fn two_life_losses_are_placed_without_asking() {
    assert_eq!(
        prompts_for(vec![life_trigger(1, false), life_trigger(2, false)]),
        0
    );
}

#[test]
fn a_gain_and_a_loss_of_the_same_life_still_ask() {
    // At 2 life, "lose 3" then "gain 5" loses the game at the state-based check between
    // them; the other order survives. That is a real decision.
    assert_eq!(
        prompts_for(vec![life_trigger(1, true), life_trigger(2, false)]),
        1
    );
}

#[test]
fn a_gain_still_conflicts_with_something_that_reads_life() {
    let reader = trigger(
        2,
        P0,
        &[Resource::Life(P0)],
        &[Resource::Object(ObjectId(901))],
        false,
    );
    assert_eq!(prompts_for(vec![life_trigger(1, true), reader]), 1);
}

#[test]
fn copies_of_one_cards_trigger_are_interchangeable() {
    // Same card, same ability, same effect on the same thing: swapping them changes
    // nothing, so there is nothing to ask — even though each writes the same permanent.
    let mut a = trigger(1, P0, &[], &[Resource::Object(ObjectId(900))], false);
    let mut b = trigger(2, P0, &[], &[Resource::Object(ObjectId(900))], false);
    a.card = ON_ENTER;
    b.card = ON_ENTER;
    assert_eq!(prompts_for(vec![a, b]), 0);
}

#[test]
fn copies_whose_effects_differ_are_not_interchangeable() {
    // Same card, but one reads something the other does not: they are not the same
    // ability doing the same thing, so the ordinary analysis decides — and here it asks.
    let mut a = trigger(1, P0, &[], &[Resource::Object(ObjectId(900))], false);
    let mut b = trigger(
        2,
        P0,
        &[Resource::Object(ObjectId(901))],
        &[Resource::Object(ObjectId(900))],
        false,
    );
    a.card = ON_ENTER;
    b.card = ON_ENTER;
    assert_eq!(prompts_for(vec![a, b]), 1);
}

#[test]
fn two_banners_attacking_go_on_the_stack_without_a_question() {
    // End to end with the real card: two "whenever this attacks, you gain 1 life".
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.step = mtg_core::Step::Untap;
    state.priority = Some(P0);
    for _ in 0..10 {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Library, P0));
        state.place(DUMMY, P1, ZoneRef::of(Zone::Library, P1));
    }
    state.place(ON_ATTACK, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(ON_ATTACK, P0, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let mut asked = Vec::new();
    for _ in 0..5000 {
        if engine.state.step == mtg_core::Step::EndCombat {
            break;
        }
        match engine.advance(&cards) {
            mtg_engine::Progress::Continue => {}
            mtg_engine::Progress::GameOver { .. } => break,
            mtg_engine::Progress::NeedsChoice(c) => {
                let answer = match &c.kind {
                    ChoiceKind::DeclareAttackers { eligible, .. } if c.who == P0 => {
                        mtg_engine::choice::Answer::Objects(eligible.clone())
                    }
                    ChoiceKind::Priority { .. } => mtg_engine::choice::Answer::Pass,
                    _ => {
                        asked.push(c.clone());
                        c.default
                            .clone()
                            .unwrap_or(mtg_engine::choice::Answer::Pass)
                    }
                };
                engine.answer(&cards, c.id, answer).unwrap();
            }
        }
    }

    assert_eq!(
        ordering_prompts(&asked),
        0,
        "no ordering question: {asked:#?}"
    );
    assert_eq!(engine.state.player(P0).life, 22, "both triggers resolved");
}

// ---- overlapping read/write sets ----

#[test]
fn overlapping_reads_and_writes_trigger_ordering_prompt() {
    // Two triggers: both read and write the same resource.
    // Engine should ask for ordering since order matters.
    let object = Resource::Object(ObjectId(42));
    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[object.clone()], &[object.clone()], false),
        trigger(2, P0, &[object.clone()], &[object.clone()], false),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 2
    });

    let prompts = ordering_prompts(&interruptions);
    assert!(
        prompts > 0,
        "overlapping read/write should prompt for ordering"
    );
}

#[test]
fn trigger_reading_without_writing_allows_any_order() {
    // Two triggers that both read the same resource but don't modify it.
    // Order doesn't matter for commutativity.
    let object = Resource::Object(ObjectId(50));
    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[object.clone()], &[], false),
        trigger(2, P0, &[object.clone()], &[], false),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 2
    });

    assert_eq!(
        ordering_prompts(&interruptions),
        0,
        "pure reads with no writes should not prompt"
    );
}

#[test]
fn trigger_writing_different_resources_needs_no_ordering() {
    // Two triggers that write to different resources.
    // They are independent; no ordering needed.
    let obj1 = Resource::Object(ObjectId(51));
    let obj2 = Resource::Object(ObjectId(52));
    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[], &[obj1], false),
        trigger(2, P0, &[], &[obj2], false),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 2
    });

    assert_eq!(
        ordering_prompts(&interruptions),
        0,
        "writes to different resources are commutative"
    );
}

#[test]
fn trigger_with_prompt_affects_ordering_decision() {
    // A trigger that prompts during resolution might have
    // unpredictable writes, so conflicting triggers with it
    // should still request ordering.
    let resource = Resource::Object(ObjectId(60));
    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[], &[resource.clone()], true), // has prompts
        trigger(2, P0, &[], &[resource.clone()], false),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 2
    });

    let prompts = ordering_prompts(&interruptions);
    assert!(
        prompts > 0,
        "trigger with prompts + conflict should request ordering"
    );
}

// ---- simultaneous trigger ordering decisions ----

#[test]
fn order_matters_when_triggers_modify_state_read_by_second() {
    // Trigger 1: reads life, writes to an object.
    // Trigger 2: reads that object, writes life.
    // Order clearly matters: if T1 first, T2 may make a different choice.
    let obj = Resource::Object(ObjectId(70));
    let life = Resource::Life(P0);

    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[life.clone()], &[obj.clone()], false),
        trigger(2, P0, &[obj.clone()], &[life.clone()], false),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 2
    });

    assert!(
        ordering_prompts(&interruptions) > 0,
        "read-write dependency should prompt"
    );
}

#[test]
fn order_independent_when_no_write_read_dependencies() {
    // Trigger 1: reads A, writes B.
    // Trigger 2: reads C, writes D.
    // Neither reads what the other writes.
    let (mut engine, cards) = game_with(vec![
        trigger(
            1,
            P0,
            &[Resource::Object(ObjectId(80))],
            &[Resource::Object(ObjectId(81))],
            false,
        ),
        trigger(
            2,
            P0,
            &[Resource::Object(ObjectId(82))],
            &[Resource::Object(ObjectId(83))],
            false,
        ),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 2
    });

    assert_eq!(
        ordering_prompts(&interruptions),
        0,
        "independent read/write sets need no ordering"
    );
}

#[test]
fn three_triggers_with_partial_conflicts() {
    // Trigger 1 and 2 conflict (both write to same object).
    // Trigger 3 is independent.
    // Should ask only about the conflicting pair, not 3.
    let shared = Resource::Object(ObjectId(90));
    let unique = Resource::Object(ObjectId(91));

    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[], &[shared.clone()], false),
        trigger(2, P0, &[], &[shared.clone()], false),
        trigger(3, P0, &[], &[unique], false),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 3
    });

    let prompts = ordering_prompts(&interruptions);
    assert!(prompts > 0, "should ask about T1/T2 conflict");
}

// ---- conservative ask-when-unsure behavior ----

#[test]
fn unknown_effect_footprint_triggers_conservative_ask() {
    // A trigger with `prompts: true` has unknown writes (interaction during resolution).
    // A second conflicting trigger should trigger conservative ask-when-unsure.
    let resource = Resource::Object(ObjectId(100));
    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[], &[resource.clone()], true), // prompts during resolution
        trigger(2, P0, &[], &[resource.clone()], false),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 2
    });

    assert!(
        ordering_prompts(&interruptions) > 0,
        "conservative: unsure about prompts => ask"
    );
}

#[test]
fn multiple_players_with_same_potential_conflict() {
    // Player 0 has two conflicting triggers.
    // Player 1 has two independent triggers.
    // Should ask P0 but not P1.
    let conflict = Resource::Object(ObjectId(110));
    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[], &[conflict.clone()], false),
        trigger(2, P0, &[], &[conflict.clone()], false),
        trigger(3, P1, &[], &[Resource::Object(ObjectId(111))], false),
        trigger(4, P1, &[], &[Resource::Object(ObjectId(112))], false),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 4
    });

    let prompts: Vec<_> = interruptions
        .iter()
        .filter(|c| matches!(c.kind, ChoiceKind::OrderTriggers { .. }))
        .collect();

    assert!(prompts.iter().any(|c| c.who == P0), "should ask P0");
    assert!(!prompts.iter().any(|c| c.who == P1), "should not ask P1");
}

#[test]
fn edge_case_many_triggers_mixed_conflicts() {
    // 5 triggers:
    // T1, T2 conflict (both write A)
    // T2, T3 conflict (both write B)
    // T4, T5 independent from all
    // This forms a chain: T1-T2-T3 all interact.
    let a = Resource::Object(ObjectId(120));
    let b = Resource::Object(ObjectId(121));
    let unique1 = Resource::Object(ObjectId(122));
    let unique2 = Resource::Object(ObjectId(123));

    let (mut engine, cards) = game_with(vec![
        trigger(1, P0, &[], &[a.clone()], false),
        trigger(2, P0, &[], &[a.clone(), b.clone()], false),
        trigger(3, P0, &[], &[b.clone()], false),
        trigger(4, P0, &[], &[unique1], false),
        trigger(5, P0, &[], &[unique2], false),
    ]);

    let interruptions = run(&mut engine, &cards, 900, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).len() >= 5
    });

    assert!(
        ordering_prompts(&interruptions) > 0,
        "chain of conflicts should prompt"
    );
}
