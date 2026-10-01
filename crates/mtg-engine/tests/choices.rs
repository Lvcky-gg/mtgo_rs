//! Choices gathered during resolution.
//!
//! Resolution cannot suspend — it is a recursive tree walk that applies events as it
//! goes — so it is restartable instead: roll back, ask, re-run with the answer. The
//! tests that matter most here are the ones proving that replay is *not* observable:
//! nothing half-applied leaks out, and nothing is applied twice.

mod common;

use common::*;
use mtg_core::{ObjectId, Step, Zone, ZoneRef};
use mtg_engine::{
    Engine, Progress,
    actions::Action,
    choice::{Answer, Choice, ChoiceKind},
    state::GameState,
};

fn board() -> GameState {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.active_player = P0;
    state.step = Step::PrecombatMain;
    state.priority = Some(P0);
    state
}

/// Drive the game, casting `want` when offered and answering questions with `reply`.
///
/// Returns every non-priority choice that was shown.
fn play(
    engine: &mut Engine,
    cards: &TestCards,
    want: Option<ObjectId>,
    mut reply: impl FnMut(&Choice) -> Option<Answer>,
    budget: usize,
    mut stop: impl FnMut(&Engine) -> bool,
) -> Vec<Choice> {
    let mut asked = Vec::new();

    for _ in 0..budget {
        if stop(engine) {
            return asked;
        }
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => return asked,
            Progress::NeedsChoice(c) => {
                let answer = match &c.kind {
                    ChoiceKind::Priority { legal } => want
                        .and_then(|w| {
                            legal
                                .actions
                                .iter()
                                .find(|a| matches!(a, Action::Cast { object } if *object == w))
                                .cloned()
                        })
                        .map(Answer::Action)
                        .unwrap_or(Answer::Pass),
                    _ => {
                        asked.push(c.clone());
                        reply(&c)
                            .or_else(|| c.default.clone())
                            .unwrap_or(Answer::Pass)
                    }
                };
                if engine.answer(cards, c.id, answer).is_err() {
                    let fallback = c.default.clone().unwrap_or(Answer::Pass);
                    let _ = engine.answer(cards, c.id, fallback);
                }
            }
        }
    }
    asked
}

fn life(engine: &Engine) -> i32 {
    engine.state.player(P0).life
}

fn hand(engine: &Engine) -> usize {
    engine.state.objects_in(ZoneRef::of(Zone::Hand, P0)).len()
}

fn graveyard(engine: &Engine) -> usize {
    engine
        .state
        .objects_in(ZoneRef::of(Zone::Graveyard, P0))
        .len()
}

/// Cast `card` from hand and settle.
fn cast_and_settle(
    card: mtg_core::CardId,
    extra: impl FnOnce(&mut GameState),
    reply: impl FnMut(&Choice) -> Option<Answer>,
) -> (Engine, TestCards, Vec<Choice>) {
    let mut state = board();
    extra(&mut state);
    let spell = state.place(card, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    let asked = play(&mut engine, &cards, Some(spell), reply, 6000, |e| {
        e.state.step == Step::BeginCombat
    });
    (engine, cards, asked)
}

// ---- "you may" ---------------------------------------------------------

#[test]
fn an_optional_effect_asks_and_declining_does_nothing() {
    let (engine, _cards, asked) = cast_and_settle(MAY_GAIN, |_| {}, |_| Some(Answer::Bool(false)));

    assert!(
        asked.iter().any(|c| matches!(c.kind, ChoiceKind::Confirm)),
        "it should have asked, got {asked:#?}"
    );
    assert_eq!(life(&engine), 20, "declining gains nothing");
}

#[test]
fn accepting_an_optional_effect_carries_it_out() {
    let (engine, _cards, _) = cast_and_settle(MAY_GAIN, |_| {}, |_| Some(Answer::Bool(true)));
    assert_eq!(life(&engine), 23);
}

// ---- modal -------------------------------------------------------------

#[test]
fn a_modal_effect_runs_the_chosen_mode() {
    let (first, _, _) = cast_and_settle(MODAL, |_| {}, |_| Some(Answer::Modes(vec![0])));
    assert_eq!(life(&first), 22, "mode 0 gains 2");

    let (second, _, _) = cast_and_settle(MODAL, |_| {}, |_| Some(Answer::Modes(vec![1])));
    assert_eq!(life(&second), 27, "mode 1 gains 7");
}

#[test]
fn a_modal_effect_asks_with_the_mode_labels() {
    let (_, _, asked) = cast_and_settle(MODAL, |_| {}, |_| None);
    let modes = asked.iter().find_map(|c| match &c.kind {
        ChoiceKind::ChooseModes {
            available, count, ..
        } => Some((available.clone(), *count)),
        _ => None,
    });
    let (available, count) = modes.expect("should ask for a mode");
    assert_eq!(count, 1);
    assert_eq!(available.len(), 2, "both modes offered");
}

// ---- discard -----------------------------------------------------------

#[test]
fn a_discard_asks_which_card_and_moves_it_to_the_graveyard() {
    let (engine, _, asked) = cast_and_settle(
        FORCED_DISCARD,
        |state| {
            for _ in 0..3 {
                state.place(DUMMY, P0, ZoneRef::of(Zone::Hand, P0));
            }
        },
        |c| match &c.kind {
            // Pick the last offered card, to prove the answer is actually used.
            ChoiceKind::ChooseObjects { from, .. } => {
                Some(Answer::Objects(from.last().copied().into_iter().collect()))
            }
            _ => None,
        },
    );

    assert!(
        asked
            .iter()
            .any(|c| matches!(c.kind, ChoiceKind::ChooseObjects { .. })),
        "should have asked which card"
    );
    assert_eq!(hand(&engine), 2, "one of three cards left the hand");
    assert_eq!(graveyard(&engine), 1);
}

#[test]
fn a_random_discard_does_not_ask() {
    // Discarding at random is not a decision, so it must not prompt.
    let (engine, _, asked) = cast_and_settle(
        RANDOM_DISCARD,
        |state| {
            for _ in 0..3 {
                state.place(DUMMY, P0, ZoneRef::of(Zone::Hand, P0));
            }
        },
        |_| None,
    );

    assert!(
        !asked
            .iter()
            .any(|c| matches!(c.kind, ChoiceKind::ChooseObjects { .. })),
        "a random discard is not a choice, got {asked:#?}"
    );
    assert_eq!(hand(&engine), 2, "but a card was still discarded");
    assert_eq!(graveyard(&engine), 1);
}

#[test]
fn a_discard_with_an_empty_hand_does_nothing_and_asks_nothing() {
    let (engine, _, asked) = cast_and_settle(FORCED_DISCARD, |_| {}, |_| None);
    assert!(
        !asked
            .iter()
            .any(|c| matches!(c.kind, ChoiceKind::ChooseObjects { .. }))
    );
    assert_eq!(graveyard(&engine), 0);
}

// ---- sacrifice ---------------------------------------------------------

#[test]
fn a_sacrifice_asks_which_permanent_and_takes_it() {
    let (engine, cards, asked) = cast_and_settle(
        SAC_ETB,
        |state| {
            let a = state.place(DUMMY, P0, ZoneRef::shared(Zone::Battlefield));
            state.objects.get_mut(&a).unwrap().summoning_sick = false;
        },
        |c| match &c.kind {
            ChoiceKind::ChooseObjects { from, .. } => {
                Some(Answer::Objects(from.first().copied().into_iter().collect()))
            }
            _ => None,
        },
    );
    let _ = cards;

    assert!(
        asked
            .iter()
            .any(|c| matches!(c.kind, ChoiceKind::ChooseObjects { .. }))
    );
    assert_eq!(
        graveyard(&engine),
        1,
        "the sacrificed creature is in the graveyard"
    );
}

// ---- a chosen number ---------------------------------------------------

#[test]
fn a_chosen_number_is_asked_within_its_bounds_and_used() {
    let (engine, _, asked) = cast_and_settle(
        CHOSEN_X,
        |_| {},
        |c| match &c.kind {
            ChoiceKind::ChooseX { .. } => Some(Answer::Number(2)),
            _ => None,
        },
    );

    let bounds = asked.iter().find_map(|c| match &c.kind {
        ChoiceKind::ChooseX { min, max } => Some((*min, *max)),
        _ => None,
    });
    assert_eq!(bounds, Some((0, 3)), "bounds come from the card");
    assert_eq!(life(&engine), 22, "the answer was used");
}

#[test]
fn a_chosen_number_is_clamped_to_its_bounds() {
    let (engine, _, _) = cast_and_settle(
        CHOSEN_X,
        |_| {},
        |c| match &c.kind {
            ChoiceKind::ChooseX { .. } => Some(Answer::Number(99)),
            _ => None,
        },
    );
    assert_eq!(life(&engine), 23, "99 is clamped to the maximum of 3");
}

// ---- the properties that make replay safe ------------------------------

#[test]
fn an_earlier_clause_is_not_applied_twice_by_the_replay() {
    // The regression this design could plausibly get wrong. "Gain 1 life, then you may
    // gain 10": the first clause runs, the question unwinds the resolution, and the
    // whole thing runs again with the answer. If the rollback were incomplete, the
    // first gain would land twice and the total would be 32 rather than 31.
    let (engine, _, _) = cast_and_settle(SEQ_MAY, |_| {}, |_| Some(Answer::Bool(true)));
    assert_eq!(
        life(&engine),
        31,
        "20 + 1 + 10, with the 1 applied exactly once"
    );
}

#[test]
fn declining_after_an_earlier_clause_still_keeps_that_clause() {
    let (engine, _, _) = cast_and_settle(SEQ_MAY, |_| {}, |_| Some(Answer::Bool(false)));
    assert_eq!(
        life(&engine),
        21,
        "the first clause happened, the optional one did not"
    );
}

#[test]
fn nothing_half_applied_is_visible_while_the_question_is_outstanding() {
    // The state is rolled back *before* the choice is handed out, so a client cannot
    // observe a spell that has done half of what it says.
    let mut state = board();
    let spell = state.place(SEQ_MAY, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let mut observed: Option<i32> = None;
    for _ in 0..6000 {
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::Confirm => {
                    // This is the moment a client would render the board.
                    observed = Some(engine.state.player(P0).life);
                    engine.answer(&cards, c.id, Answer::Bool(true)).unwrap();
                    break;
                }
                ChoiceKind::Priority { legal } => {
                    let cast = legal
                        .actions
                        .iter()
                        .find(|a| matches!(a, Action::Cast { object } if *object == spell))
                        .cloned();
                    let a = cast.map(Answer::Action).unwrap_or(Answer::Pass);
                    engine.answer(&cards, c.id, a).unwrap();
                }
                _ => {
                    let a = c.default.clone().unwrap_or(Answer::Pass);
                    engine.answer(&cards, c.id, a).unwrap();
                }
            },
        }
    }

    assert_eq!(
        observed,
        Some(20),
        "while the question is outstanding the resolution must be rolled back entirely"
    );
}

#[test]
fn the_log_does_not_keep_the_rolled_back_events() {
    // Rollback truncates the log too, otherwise a replay would diverge from the state.
    let mut state = board();
    let spell = state.place(SEQ_MAY, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let mut log_at_question = 0usize;
    for _ in 0..6000 {
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::Confirm => {
                    log_at_question = engine.log.len();
                    engine.answer(&cards, c.id, Answer::Bool(false)).unwrap();
                    break;
                }
                ChoiceKind::Priority { legal } => {
                    let cast = legal
                        .actions
                        .iter()
                        .find(|a| matches!(a, Action::Cast { object } if *object == spell))
                        .cloned();
                    let a = cast.map(Answer::Action).unwrap_or(Answer::Pass);
                    engine.answer(&cards, c.id, a).unwrap();
                }
                _ => {
                    let a = c.default.clone().unwrap_or(Answer::Pass);
                    engine.answer(&cards, c.id, a).unwrap();
                }
            },
        }
    }

    let no_life_events = engine.log[..log_at_question]
        .iter()
        .filter(|e| matches!(e.event, mtg_core::Event::LifeChanged { .. }))
        .count();
    assert_eq!(
        no_life_events, 0,
        "the rolled-back life gain must not be left in the log"
    );
}
