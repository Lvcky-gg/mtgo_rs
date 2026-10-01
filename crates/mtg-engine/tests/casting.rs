//! Casting spells: cost determination, affordability, and payment.

mod common;

use common::*;
use mtg_core::{Step, Zone, ZoneRef};
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
    state.step = Step::Untap;
    state.priority = Some(P0);
    for _ in 0..10 {
        state.place(DUMMY, P0, ZoneRef::of(Zone::Library, P0));
        state.place(DUMMY, P1, ZoneRef::of(Zone::Library, P1));
    }
    state
}

/// Whether `actions` offers casting this specific object.
///
/// Matters because the test library is stocked with free-costed cards, one of which
/// is drawn on the way to the main phase — so "is any Cast offered?" is not the
/// question these tests mean to ask.
fn can_cast(actions: &[Action], object: mtg_core::ObjectId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Cast { object: o } if *o == object))
}

/// Drive to P0's precombat main phase and return the actions available there.
fn actions_in_main(engine: &mut Engine, cards: &TestCards) -> Vec<Action> {
    for _ in 0..5000 {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::Priority { legal }
                    if c.who == P0 && engine.state.step == Step::PrecombatMain =>
                {
                    return legal.actions.clone();
                }
                _ => {
                    engine
                        .answer(cards, c.id, c.default.clone().unwrap_or(Answer::Pass))
                        .unwrap();
                }
            },
        }
    }
    Vec::new()
}

/// Drive to P0's main phase and cast one specific object.
fn cast_in_main(engine: &mut Engine, cards: &TestCards, want: mtg_core::ObjectId) -> bool {
    for _ in 0..5000 {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
                let cast = match &c.kind {
                    ChoiceKind::Priority { legal } if c.who == P0 => legal
                        .actions
                        .iter()
                        .find(|a| matches!(a, Action::Cast { object } if *object == want))
                        .cloned(),
                    _ => None,
                };
                match cast {
                    Some(a) => {
                        engine.answer(cards, c.id, Answer::Action(a)).unwrap();
                        return true;
                    }
                    None => {
                        engine
                            .answer(cards, c.id, c.default.clone().unwrap_or(Answer::Pass))
                            .unwrap();
                    }
                }
            }
        }
    }
    false
}

fn untapped_lands(engine: &Engine) -> usize {
    engine
        .state
        .battlefield()
        .iter()
        .filter(|id| engine.state.objects.get(id).is_some_and(|o| !o.tapped))
        .count()
}

// ---- affordability gates the legal action list -------------------------

#[test]
fn a_spell_is_offered_once_its_cost_can_be_paid() {
    // {1}{W} with two white sources.
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let spell = state.place(COSTED, P0, ZoneRef::of(Zone::Hand, P0));

    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    let actions = actions_in_main(&mut engine, &cards);

    assert!(
        can_cast(&actions, spell),
        "a payable spell should be castable, got {actions:?}"
    );
}

#[test]
fn a_spell_is_not_offered_when_it_cannot_be_paid() {
    // {1}{W} with only one source.
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let spell = state.place(COSTED, P0, ZoneRef::of(Zone::Hand, P0));

    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    let actions = actions_in_main(&mut engine, &cards);

    assert!(!can_cast(&actions, spell), "one land cannot pay {{1}}{{W}}");
}

#[test]
fn one_dual_source_cannot_pay_a_two_colour_cost() {
    // The matching case, end to end: {W}{G} needs two mana, and a single "white or
    // green" land makes one.
    let mut state = board();
    state.place(DUAL_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let spell = state.place(TWO_COLOR, P0, ZoneRef::of(Zone::Hand, P0));

    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    let actions = actions_in_main(&mut engine, &cards);

    assert!(
        !can_cast(&actions, spell),
        "one dual land must not pay {{W}}{{G}}"
    );
}

#[test]
fn a_dual_source_plus_a_mono_source_pays_a_two_colour_cost() {
    // White land covers {W}; the dual land is pushed onto green.
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(DUAL_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let spell = state.place(TWO_COLOR, P0, ZoneRef::of(Zone::Hand, P0));

    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    let actions = actions_in_main(&mut engine, &cards);

    assert!(
        can_cast(&actions, spell),
        "white + dual should pay {{W}}{{G}}, got {actions:?}"
    );
}

#[test]
fn a_tapped_land_is_not_an_available_source() {
    let mut state = board();
    let a = state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(COSTED, P0, ZoneRef::of(Zone::Hand, P0));
    // Tapped *and* prevented from untapping, so the untap step does not undo this.
    state.objects.get_mut(&a).unwrap().tapped = true;
    state.active_player = P1;
    state.turn = 3;

    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // P1's turn, so P0 has no sorcery-speed window; check the source scan directly.
    let sources = mtg_engine::mana::mana_sources(&engine.state, &cards, P0);
    assert_eq!(sources.len(), 1, "the tapped land should not be a source");
    let _ = engine.advance(&cards);
}

#[test]
fn another_players_lands_are_not_available_to_you() {
    let mut state = board();
    state.place(WHITE_SOURCE, P1, ZoneRef::shared(Zone::Battlefield));
    state.place(WHITE_SOURCE, P1, ZoneRef::shared(Zone::Battlefield));
    state.place(COSTED, P0, ZoneRef::of(Zone::Hand, P0));

    let engine = Engine::new(state);
    let cards = TestCards::default();

    assert!(mtg_engine::mana::mana_sources(&engine.state, &cards, P0).is_empty());
    assert_eq!(
        mtg_engine::mana::mana_sources(&engine.state, &cards, P1).len(),
        2
    );
}

// ---- paying actually happens -------------------------------------------

#[test]
fn casting_taps_the_lands_and_resolves_the_spell() {
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let spell = state.place(COSTED, P0, ZoneRef::of(Zone::Hand, P0));

    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    assert!(
        cast_in_main(&mut engine, &cards, spell),
        "should have found a Cast action"
    );
    assert_eq!(
        untapped_lands(&engine),
        0,
        "both lands should be tapped to pay {{1}}{{W}}"
    );

    // Let it resolve.
    run(&mut engine, &cards, 2000, |e| {
        e.state.objects_in(ZoneRef::shared(Zone::Stack)).is_empty()
            && e.state.step != Step::PrecombatMain
    });

    let creatures = engine
        .state
        .battlefield()
        .iter()
        .filter(|id| {
            mtg_engine::layers::compute(&engine.state, &cards, **id)
                .is_some_and(|c| c.has_type(mtg_core::CardType::Creature))
        })
        .count();
    assert!(
        creatures >= 1,
        "the creature spell should have resolved onto the battlefield"
    );
}

#[test]
fn the_mana_pool_is_empty_again_once_the_spell_is_paid_for() {
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let spell = state.place(COSTED, P0, ZoneRef::of(Zone::Hand, P0));

    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    assert!(cast_in_main(&mut engine, &cards, spell));

    assert_eq!(
        engine.state.player(P0).mana.total(),
        0,
        "mana produced to pay a cost should all be spent"
    );
}

#[test]
fn unspent_mana_empties_at_the_end_of_a_step() {
    // CR 500.4.
    let mut state = board();
    state
        .players
        .get_mut(&P0)
        .unwrap()
        .mana
        .add(Some(mtg_core::Color::White), 3);

    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    run(&mut engine, &cards, 2000, |e| {
        e.state.step == Step::PostcombatMain
    });

    assert_eq!(
        engine.state.player(P0).mana.total(),
        0,
        "pools empty between steps"
    );
}

// ---- mana abilities stay out of the auto-pass decision -----------------

#[test]
fn mana_abilities_do_not_count_as_something_worth_stopping_for() {
    // If mana abilities sat in `actions`, `is_only_passing` would be false for
    // anyone controlling an untapped land, and auto-pass would never fire again.
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    state.active_player = P1;
    state.priority = Some(P0);
    state.step = Step::Upkeep;

    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    for _ in 0..2000 {
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
                if let ChoiceKind::Priority { legal } = &c.kind
                    && c.who == P0
                {
                    assert!(
                        legal.is_only_passing(),
                        "holding two untapped lands and nothing castable is not a decision"
                    );
                    assert_eq!(
                        legal.mana_abilities.len(),
                        2,
                        "but the abilities are still offered"
                    );
                    return;
                }
                engine
                    .answer(&cards, c.id, c.default.clone().unwrap_or(Answer::Pass))
                    .unwrap();
            }
        }
    }
    panic!("never reached a priority choice for P0");
}

// ---- tapping by hand ----------------------------------------------------

/// Drive to P0's first priority in the precombat main phase, and return that choice.
fn main_phase_priority(engine: &mut Engine, cards: &TestCards) -> mtg_engine::Choice {
    for _ in 0..5000 {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
                if matches!(c.kind, ChoiceKind::Priority { .. })
                    && c.who == P0
                    && engine.state.step == Step::PrecombatMain
                {
                    return c;
                }
                engine
                    .answer(cards, c.id, c.default.clone().unwrap_or(Answer::Pass))
                    .unwrap();
            }
        }
    }
    panic!("never reached P0's main phase");
}

fn mana_abilities(choice: &mtg_engine::Choice) -> Vec<Action> {
    match &choice.kind {
        ChoiceKind::Priority { legal } => legal.mana_abilities.clone(),
        _ => Vec::new(),
    }
}

#[test]
fn a_dual_source_is_offered_once_per_colour() {
    let mut state = board();
    state.place(DUAL_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let colors: Vec<_> = mana_abilities(&main_phase_priority(&mut engine, &cards))
        .into_iter()
        .map(|a| match a {
            Action::ActivateManaAbility { color, .. } => color,
            other => panic!("not a mana activation: {other:?}"),
        })
        .collect();
    assert_eq!(
        colors,
        vec![Some(mtg_core::Color::White), Some(mtg_core::Color::Green)]
    );
}

#[test]
fn tapping_a_land_by_hand_floats_its_mana_and_keeps_priority() {
    let mut state = board();
    state.place(DUAL_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let choice = main_phase_priority(&mut engine, &cards);
    let green = mana_abilities(&choice)
        .into_iter()
        .find(|a| {
            matches!(
                a,
                Action::ActivateManaAbility {
                    color: Some(mtg_core::Color::Green),
                    ..
                }
            )
        })
        .expect("green offered");
    engine
        .answer(&cards, choice.id, Answer::Action(green))
        .unwrap();

    assert_eq!(untapped_lands(&engine), 0, "the land is tapped");
    assert_eq!(
        engine.state.player(P0).mana.amounts[mtg_core::Color::Green as usize],
        1
    );
    assert_eq!(
        engine.state.player(P0).mana.total(),
        1,
        "exactly the chosen colour"
    );
    assert!(
        engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty(),
        "a mana ability uses no stack (CR 605.3)"
    );

    // Priority comes straight back, in the same step: activating a mana ability is not a pass.
    loop {
        match engine.advance(&cards) {
            Progress::Continue => continue,
            Progress::NeedsChoice(c) => {
                assert!(matches!(c.kind, ChoiceKind::Priority { .. }));
                assert_eq!(c.who, P0);
                assert_eq!(engine.state.step, Step::PrecombatMain);
                assert!(
                    mana_abilities(&c).is_empty(),
                    "a tapped land offers nothing more"
                );
                break;
            }
            Progress::GameOver { .. } => panic!("game ended"),
        }
    }
}

#[test]
fn a_tapped_land_is_not_offered_and_an_illegal_tap_is_rejected() {
    let mut state = board();
    state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let choice = main_phase_priority(&mut engine, &cards);
    let tap = mana_abilities(&choice).remove(0);
    engine
        .answer(&cards, choice.id, Answer::Action(tap.clone()))
        .unwrap();

    let again = loop {
        match engine.advance(&cards) {
            Progress::NeedsChoice(c) => break c,
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended"),
        }
    };
    assert!(
        engine
            .answer(&cards, again.id, Answer::Action(tap))
            .is_err(),
        "cannot tap twice"
    );
    assert_eq!(
        engine.state.player(P0).mana.total(),
        1,
        "and no second mana appeared"
    );
}

#[test]
fn floating_mana_is_spent_before_anything_else_is_tapped() {
    let mut state = board();
    for _ in 0..3 {
        state.place(WHITE_SOURCE, P0, ZoneRef::shared(Zone::Battlefield));
    }
    let spell = state.place(COSTED, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // Float one white by hand, then cast the {1}{W} spell.
    let choice = main_phase_priority(&mut engine, &cards);
    let tap = mana_abilities(&choice).remove(0);
    engine
        .answer(&cards, choice.id, Answer::Action(tap))
        .unwrap();
    assert!(cast_in_main(&mut engine, &cards, spell));

    assert_eq!(
        engine.state.player(P0).mana.total(),
        0,
        "the floating mana was used"
    );
    assert_eq!(
        untapped_lands(&engine),
        1,
        "so only one more land was tapped, not two"
    );
}
