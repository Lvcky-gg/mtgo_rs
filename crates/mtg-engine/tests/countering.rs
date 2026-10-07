//! Countering (CR 701.5).

mod common;

use common::*;
use mtg_core::{Event, ObjectId, Step, Zone, ZoneRef};
use mtg_engine::{
    Engine, Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
    state::GameState,
};

fn board() -> GameState {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.step = Step::PrecombatMain;
    state.priority = Some(P0);
    // Enough mana that the costed spells in these tests are actually castable.
    for _ in 0..6 {
        state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    }
    state
}

fn stack(engine: &Engine) -> Vec<ObjectId> {
    engine.state.objects_in(ZoneRef::shared(Zone::Stack))
}

/// Cast each of `sequence` in order, holding priority between them and picking all legal
/// targets.
///
/// Stops at the first priority choice *after* the last cast, **without answering it** —
/// which is what leaves the stack intact for inspection. Answering a Cast action is not
/// the end of casting: the announcement still has to ask for targets, and stopping
/// there leaves the spell on the stack with no targets recorded. Leaving the choice
/// pending is safe because `advance` re-emits an outstanding choice idempotently, so the
/// next driver call sees it again.
fn cast_each(engine: &mut Engine, cards: &TestCards, sequence: &[ObjectId]) {
    let mut remaining: Vec<ObjectId> = sequence.to_vec();

    for _ in 0..6000 {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => return,
            Progress::NeedsChoice(c) => {
                let answer = match &c.kind {
                    ChoiceKind::Priority { legal } => {
                        // Everything cast and the announcements finished.
                        if remaining.is_empty() {
                            return;
                        }
                        let want = remaining[0];
                        match legal
                            .actions
                            .iter()
                            .find(|a| matches!(a, Action::Cast { object } if *object == want))
                        {
                            Some(a) => {
                                remaining.remove(0);
                                Answer::Action(a.clone())
                            }
                            None => Answer::Pass,
                        }
                    }
                    ChoiceKind::ChooseTargets { slots, .. } => {
                        Answer::Targets(slots.iter().map(|s| s.to_vec()).collect())
                    }
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                if engine.answer(cards, c.id, answer).is_err() {
                    let f = c.default.clone().unwrap_or(Answer::Pass);
                    let _ = engine.answer(cards, c.id, f);
                }
            }
        }
    }
}

/// Let everything on the stack resolve.
fn settle(engine: &mut Engine, cards: &TestCards) {
    for _ in 0..6000 {
        if stack(engine).is_empty() && engine.state.step != Step::PrecombatMain {
            return;
        }
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => return,
            Progress::NeedsChoice(c) => {
                let a = c.default.clone().unwrap_or(Answer::Pass);
                if engine.answer(cards, c.id, a).is_err() {
                    return;
                }
            }
        }
    }
}

fn legal_now(engine: &mut Engine, cards: &TestCards) -> Vec<Action> {
    for _ in 0..4000 {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::Priority { legal } if c.who == P0 => return legal.actions.clone(),
                _ => {
                    let a = c.default.clone().unwrap_or(Answer::Pass);
                    let _ = engine.answer(cards, c.id, a);
                }
            },
        }
    }
    Vec::new()
}

fn can_cast(actions: &[Action], object: ObjectId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Cast { object: o } if *o == object))
}

// ---- when a counterspell can be cast -----------------------------------

#[test]
fn a_counterspell_cannot_be_cast_with_an_empty_stack() {
    let mut state = board();
    let deny = state.place(COUNTER, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let actions = legal_now(&mut engine, &cards);
    assert!(!can_cast(&actions, deny), "there is no spell to counter");
}

#[test]
fn a_counterspell_becomes_castable_once_a_spell_is_on_the_stack() {
    let mut state = board();
    let gain = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let deny = state.place(COUNTER, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    cast_each(&mut engine, &cards, &[gain]);
    assert_eq!(stack(&engine).len(), 1);

    let actions = legal_now(&mut engine, &cards);
    assert!(
        can_cast(&actions, deny),
        "a spell is on the stack, so it can be countered"
    );
}

#[test]
fn a_counterspell_cannot_target_an_ability_on_the_stack() {
    // CR 112.1 — an ability on the stack is not a spell.
    let mut state = board();
    let deny = state.place(COUNTER, P0, ZoneRef::of(Zone::Hand, P0));
    // A triggered ability, placed on the stack by the engine.
    let src = state.place(ON_UPKEEP, P0, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&src).unwrap().summoning_sick = false;
    state.step = Step::Untap;
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // Advance until the upkeep trigger is on the stack.
    for _ in 0..600 {
        if !stack(&engine).is_empty() {
            break;
        }
        match engine.advance(&cards) {
            Progress::NeedsChoice(c) => {
                let a = c.default.clone().unwrap_or(Answer::Pass);
                let _ = engine.answer(&cards, c.id, a);
            }
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
        }
    }
    assert_eq!(stack(&engine).len(), 1, "the trigger is on the stack");

    let actions = legal_now(&mut engine, &cards);
    assert!(
        !can_cast(&actions, deny),
        "only an ability is on the stack, and that is not a spell"
    );
}

// ---- countering works --------------------------------------------------

#[test]
fn a_countered_spell_does_not_resolve() {
    let mut state = board();
    let gain = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let deny = state.place(COUNTER, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // Both on the stack, the counterspell on top.
    cast_each(&mut engine, &cards, &[gain, deny]);
    assert_eq!(stack(&engine).len(), 2, "both spells are on the stack");

    settle(&mut engine, &cards);

    assert_eq!(
        engine.state.player(P0).life,
        20,
        "the life-gain spell was countered, so it gained nothing"
    );
}

#[test]
fn countering_logs_a_countered_event() {
    let mut state = board();
    let gain = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let deny = state.place(COUNTER, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    cast_each(&mut engine, &cards, &[gain, deny]);
    settle(&mut engine, &cards);

    assert!(
        engine
            .log
            .iter()
            .any(|e| matches!(e.event, Event::Countered { .. })),
        "a Countered event should be logged"
    );
}

#[test]
fn a_countered_spell_goes_to_its_owners_graveyard() {
    // CR 701.5a.
    let mut state = board();
    let gain = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let deny = state.place(COUNTER, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    cast_each(&mut engine, &cards, &[gain, deny]);
    settle(&mut engine, &cards);

    // Both the countered spell and the counterspell itself end up there.
    assert_eq!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .len(),
        2,
        "the countered spell and the counterspell are both in the graveyard"
    );
    assert!(stack(&engine).is_empty());
}

#[test]
fn an_uncountered_spell_still_resolves_normally() {
    // The control: without the counterspell, the life gain happens.
    let mut state = board();
    let gain = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    cast_each(&mut engine, &cards, &[gain]);
    settle(&mut engine, &cards);

    assert_eq!(engine.state.player(P0).life, 21);
}

// ---- can't be countered ------------------------------------------------

#[test]
fn a_spell_that_cannot_be_countered_survives() {
    use mtg_engine::state::{AffectedSet, ContinuousEffect};
    use mtg_ir::effect::{Duration, Modification, Restriction};

    let mut state = board();
    let gain = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let deny = state.place(COUNTER, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    cast_each(&mut engine, &cards, &[gain]);
    let on_stack = stack(&engine)[0];

    // Make that spell uncounterable while it sits on the stack.
    let id = engine.state.new_object_id();
    let timestamp = engine.state.bump();
    engine.state.continuous.push(ContinuousEffect {
        id,
        source: on_stack,
        affected: AffectedSet::Fixed(vec![on_stack]),
        modification: Modification::Restriction(Restriction::CantBeCountered),
        duration: Duration::Permanent,
        timestamp,
        layer: mtg_engine::layers::layer::ABILITY,
        ability: None,
        controller: None,
    });

    cast_each(&mut engine, &cards, &[deny]);
    settle(&mut engine, &cards);

    assert_eq!(
        engine.state.player(P0).life,
        21,
        "the spell could not be countered, so it resolved"
    );
    assert!(
        !engine
            .log
            .iter()
            .any(|e| matches!(e.event, Event::Countered { .. })),
        "and nothing was countered"
    );
}

// ---- interaction with the stack ---------------------------------------

#[test]
fn a_counterspell_can_be_countered_by_another_one() {
    // Three deep: gain, deny, deny. LIFO means the second Deny resolves first and
    // counters the first Deny, so the life gain survives.
    let mut state = board();
    let gain = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let deny_a = state.place(COUNTER, P0, ZoneRef::of(Zone::Hand, P0));
    let deny_b = state.place(COUNTER, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    cast_each(&mut engine, &cards, &[gain, deny_a, deny_b]);
    assert_eq!(stack(&engine).len(), 3, "three spells on the stack");

    settle(&mut engine, &cards);

    assert_eq!(
        engine.state.player(P0).life,
        21,
        "the second counterspell countered the first, so the life gain resolved"
    );
}

#[test]
fn a_counterspell_whose_target_already_left_the_stack_fizzles() {
    // If the targeted spell resolves first, the counterspell has no legal target on
    // resolution and is countered by the game rules (CR 608.2b) — the same path
    // fizzling already uses.
    let mut state = board();
    let gain = state.place(SMALL_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let deny = state.place(COUNTER, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    cast_each(&mut engine, &cards, &[gain, deny]);
    let stack_now = stack(&engine);
    let target = *stack_now
        .last()
        .expect("bottom of the stack is the gain spell");

    // Remove the target from under the counterspell, as though it had already resolved.
    engine.state.objects.remove(&target);
    if let Some(order) = engine
        .state
        .zone_order
        .get_mut(&ZoneRef::shared(Zone::Stack))
    {
        order.retain(|o| *o != target);
    }

    settle(&mut engine, &cards);

    assert!(
        engine
            .log
            .iter()
            .any(|e| matches!(e.event, Event::Countered { .. })),
        "the counterspell itself should have been countered by the game rules"
    );
    assert!(stack(&engine).is_empty());
}
