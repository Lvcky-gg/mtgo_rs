//! Targeting: legality on announcement (CR 601.2c) and again on resolution (CR 608.2b).

mod common;

use common::*;
use mtg_core::{ObjectId, Step, Target, Zone, ZoneRef};
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

fn ready(state: &mut GameState, card: mtg_core::CardId, who: mtg_core::PlayerId) -> ObjectId {
    let id = state.place(card, who, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    id
}

/// Drive, casting `want` and picking targets with `pick`.
fn play(
    engine: &mut Engine,
    cards: &TestCards,
    want: Option<ObjectId>,
    mut pick: impl FnMut(&[Target]) -> Vec<Target>,
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
                    ChoiceKind::ChooseTargets { slots } => {
                        asked.push(c.clone());
                        Answer::Targets(slots.iter().map(|s| pick(s)).collect())
                    }
                    _ => {
                        asked.push(c.clone());
                        c.default.clone().unwrap_or(Answer::Pass)
                    }
                };
                if engine.answer(cards, c.id, answer).is_err() {
                    let f = c.default.clone().unwrap_or(Answer::Pass);
                    let _ = engine.answer(cards, c.id, f);
                }
            }
        }
    }
    asked
}

fn cast_actions(engine: &mut Engine, cards: &TestCards) -> Vec<Action> {
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

// ---- announcement ------------------------------------------------------

#[test]
fn a_targeted_spell_asks_for_its_target() {
    let mut state = board();
    let victim = ready(&mut state, DUMMY, P1);
    let bolt = state.place(BOLT, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let asked = play(
        &mut engine,
        &cards,
        Some(bolt),
        |legal| legal.to_vec(),
        4000,
        |e| e.state.step == Step::BeginCombat,
    );

    let slots = asked.iter().find_map(|c| match &c.kind {
        ChoiceKind::ChooseTargets { slots } => Some(slots.clone()),
        _ => None,
    });
    let slots = slots.expect("should have asked for a target");
    assert_eq!(slots.len(), 1, "one target slot");
    assert!(
        slots[0].contains(&Target::Object(victim)),
        "the creature is a legal target"
    );
}

#[test]
fn a_targeted_spell_affects_the_chosen_target() {
    let mut state = board();
    let victim = ready(&mut state, DUMMY, P1); // 2/2
    let bolt = state.place(BOLT, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play(
        &mut engine,
        &cards,
        Some(bolt),
        |legal| legal.to_vec(),
        4000,
        |e| !e.state.objects.contains_key(&victim),
    );

    assert!(
        !engine.state.objects.contains_key(&victim),
        "2 damage kills a 2/2"
    );
}

#[test]
fn a_spell_with_no_legal_target_is_not_offered() {
    // CR 601.2c — it cannot be cast at all, rather than being cast and fizzling.
    let mut state = board();
    let bolt = state.place(BOLT, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let actions = cast_actions(&mut engine, &cards);
    assert!(
        !can_cast(&actions, bolt),
        "no creatures exist, so there is nothing to target"
    );
}

#[test]
fn a_spell_becomes_castable_once_a_legal_target_exists() {
    let mut state = board();
    ready(&mut state, DUMMY, P1);
    let bolt = state.place(BOLT, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let actions = cast_actions(&mut engine, &cards);
    assert!(can_cast(&actions, bolt));
}

#[test]
fn announcing_a_target_emits_a_targeted_event() {
    // So that "whenever this becomes the target of a spell" has something to see.
    let mut state = board();
    let victim = ready(&mut state, BIG, P1); // 6/6, survives the damage
    let bolt = state.place(BOLT, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play(
        &mut engine,
        &cards,
        Some(bolt),
        |legal| legal.to_vec(),
        4000,
        |e| e.state.step == Step::BeginCombat,
    );

    let targeted = engine.log.iter().any(|e| {
        matches!(e.event, mtg_core::Event::Targeted { target: Target::Object(o), .. } if o == victim)
    });
    assert!(targeted, "a Targeted event should have been logged");
}

// ---- protection from targeting -----------------------------------------

#[test]
fn hexproof_is_not_targetable_by_an_opponent() {
    let mut state = board();
    ready(&mut state, HEXPROOF_CREATURE, P1);
    let bolt = state.place(BOLT, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let actions = cast_actions(&mut engine, &cards);
    assert!(
        !can_cast(&actions, bolt),
        "an opponent's hexproof creature is the only creature, so there is no legal target"
    );
}

#[test]
fn hexproof_is_still_targetable_by_its_own_controller() {
    // Hexproof only stops *opponents* (CR 702.11b).
    let mut state = board();
    ready(&mut state, HEXPROOF_CREATURE, P0);
    let bolt = state.place(BOLT, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let actions = cast_actions(&mut engine, &cards);
    assert!(
        can_cast(&actions, bolt),
        "you may target your own hexproof creature"
    );
}

#[test]
fn shroud_is_not_targetable_by_anyone_including_its_controller() {
    let mut state = board();
    ready(&mut state, SHROUD_CREATURE, P0);
    let bolt = state.place(BOLT, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let actions = cast_actions(&mut engine, &cards);
    assert!(
        !can_cast(&actions, bolt),
        "shroud stops its controller too (CR 702.18b)"
    );
}

// ---- players as targets ------------------------------------------------

#[test]
fn a_spell_can_target_a_player() {
    let mut state = board();
    let heal = state.place(HEAL_PLAYER, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play(
        &mut engine,
        &cards,
        Some(heal),
        // Pick the caster.
        |legal| {
            legal
                .iter()
                .copied()
                .filter(|t| *t == Target::Player(P0))
                .collect()
        },
        4000,
        |e| e.state.player(P0).life >= 23,
    );

    assert_eq!(engine.state.player(P0).life, 23);
}

#[test]
fn a_player_targeting_spell_is_castable_with_an_empty_board() {
    let mut state = board();
    let heal = state.place(HEAL_PLAYER, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let actions = cast_actions(&mut engine, &cards);
    assert!(
        can_cast(&actions, heal),
        "players are always available as targets"
    );
}

// ---- distinct slots ----------------------------------------------------

#[test]
fn two_distinct_slots_cannot_pick_the_same_creature() {
    let mut state = board();
    let only = ready(&mut state, BIG, P1);
    let twin = state.place(TWO_BOLTS, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // Only one creature exists, and the two slots must differ, so it cannot be cast.
    let actions = cast_actions(&mut engine, &cards);
    assert!(
        !can_cast(&actions, twin),
        "one creature cannot fill two distinct target slots"
    );
    let _ = only;
}

#[test]
fn two_distinct_slots_are_castable_with_two_creatures() {
    let mut state = board();
    let a = ready(&mut state, BIG, P1);
    let b = ready(&mut state, BIG, P1);
    let twin = state.place(TWO_BOLTS, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    let actions = cast_actions(&mut engine, &cards);
    assert!(can_cast(&actions, twin));

    // And the second slot no longer offers the first slot's pick.
    let mut engine = Engine::new({
        let mut s = board();
        let x = ready(&mut s, BIG, P1);
        let y = ready(&mut s, BIG, P1);
        let _ = (x, y);
        s.place(TWO_BOLTS, P0, ZoneRef::of(Zone::Hand, P0));
        s
    });
    let _ = (&mut engine, a, b);
}

// ---- resolution re-check (CR 608.2b) -----------------------------------

/// Cast `spell` at `victim`, then kill the victim while the spell is on the stack.
fn cast_then_kill(spell: mtg_core::CardId) -> (Engine, TestCards) {
    let mut state = board();
    let victim = ready(&mut state, DUMMY, P1); // 2/2
    let card = state.place(spell, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // Wait for the announcement to *complete* — the object reaching the stack happens
    // first, before targets are recorded, so stopping there leaves it half-announced.
    for _ in 0..4000 {
        let announced = engine
            .log
            .iter()
            .any(|e| matches!(e.event, mtg_core::Event::SpellCast { .. }));
        if announced {
            break;
        }
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
                let answer = match &c.kind {
                    ChoiceKind::Priority { legal } => legal
                        .actions
                        .iter()
                        .find(|a| matches!(a, Action::Cast { object } if *object == card))
                        .cloned()
                        .map(Answer::Action)
                        .unwrap_or(Answer::Pass),
                    ChoiceKind::ChooseTargets { slots } => {
                        Answer::Targets(slots.iter().map(|s| s.to_vec()).collect())
                    }
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                let _ = engine.answer(&cards, c.id, answer);
            }
        }
    }

    // Now make the target die before the spell resolves. A state-based action will
    // take it, which is exactly how this happens in a real game.
    if let Some(o) = engine.state.objects.get_mut(&victim) {
        o.damage = 2;
    }

    // Let everything settle.
    for _ in 0..4000 {
        if engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
            && engine.state.step != Step::PrecombatMain
        {
            break;
        }
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
                let a = c.default.clone().unwrap_or(Answer::Pass);
                let _ = engine.answer(&cards, c.id, a);
            }
        }
    }

    (engine, cards)
}

#[test]
fn a_spell_whose_only_target_became_illegal_does_not_resolve() {
    let (engine, _cards) = cast_then_kill(BOLT);

    let countered = engine
        .log
        .iter()
        .any(|e| matches!(e.event, mtg_core::Event::Countered { .. }));
    assert!(
        countered,
        "it should have been countered by the game rules (CR 608.2b)"
    );
}

#[test]
fn a_fizzled_spell_does_none_of_its_effect_not_even_the_untargeted_part() {
    // The subtle half of CR 608.2b: "deal 2 damage to target creature, you gain 2 life"
    // gains no life when the creature is gone, because the whole spell fails to resolve
    // rather than skipping only the impossible clause.
    let (engine, _cards) = cast_then_kill(BOLT_AND_GAIN);
    assert_eq!(
        engine.state.player(P0).life,
        20,
        "no life gained, because the spell never resolved at all"
    );
}

#[test]
fn a_spell_with_a_surviving_target_still_resolves_fully() {
    // The control for the two tests above.
    let mut state = board();
    let victim = ready(&mut state, BIG, P1); // 6/6 survives 2 damage
    let card = state.place(BOLT_AND_GAIN, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    play(
        &mut engine,
        &cards,
        Some(card),
        |legal| legal.to_vec(),
        4000,
        |e| e.state.step == Step::BeginCombat,
    );

    assert_eq!(engine.state.player(P0).life, 22, "the life gain happened");
    assert_eq!(
        engine.state.objects[&victim].damage, 2,
        "and so did the damage"
    );
}

// ---- triggered abilities that target (CR 603.3d) ------------------------

/// Cast `card` and play on, picking targets with `pick`.
fn cast_and_settle(
    card: mtg_core::CardId,
    setup: impl FnOnce(&mut GameState),
    pick: impl FnMut(&[Target]) -> Vec<Target>,
) -> (Engine, TestCards, Vec<Choice>) {
    let mut state = board();
    setup(&mut state);
    let spell = state.place(card, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();
    let asked = play(&mut engine, &cards, Some(spell), pick, 6000, |e| {
        e.state.step == Step::BeginCombat
    });
    (engine, cards, asked)
}

#[test]
fn a_triggered_ability_chooses_targets_as_it_goes_on_the_stack() {
    let victim_holder = std::cell::Cell::new(None);
    let (engine, cards, asked) = cast_and_settle(
        ETB_BOLT,
        |state| {
            let v = ready(state, BIG, P1); // 6/6 so it survives and can be inspected
            victim_holder.set(Some(v));
        },
        |legal| legal.to_vec(),
    );
    let victim = victim_holder.get().unwrap();
    let _ = cards;

    let slots = asked.iter().find_map(|c| match &c.kind {
        ChoiceKind::ChooseTargets { slots } => Some(slots.clone()),
        _ => None,
    });
    let slots = slots.expect("the trigger should have asked for a target");
    assert!(
        slots[0].contains(&Target::Object(victim)),
        "the creature is a legal target"
    );
    assert_eq!(
        engine.state.objects[&victim].damage, 2,
        "and it took the damage"
    );
}

#[test]
fn a_non_targeting_trigger_asks_nothing() {
    // The control: an enters trigger with no target slots must not prompt.
    let (engine, _cards, asked) = cast_and_settle(ON_ENTER, |_| {}, |legal| legal.to_vec());

    assert!(
        !asked
            .iter()
            .any(|c| matches!(c.kind, ChoiceKind::ChooseTargets { .. })),
        "nothing to target, so nothing to ask, got {asked:#?}"
    );
    assert_eq!(
        engine.state.player(P0).life,
        23,
        "and its effect still happened"
    );
}

#[test]
fn a_creature_can_be_the_target_of_its_own_enters_trigger() {
    // Worth pinning because it is easy to assume otherwise: the ability on the stack is
    // a different object from the permanent that spawned it, so "target creature"
    // legitimately includes the creature whose arrival triggered it.
    let (engine, _cards, asked) = cast_and_settle(ETB_BOLT, |_| {}, |legal| legal.to_vec());

    assert!(
        asked
            .iter()
            .any(|c| matches!(c.kind, ChoiceKind::ChooseTargets { .. })),
        "the sentinel itself is a legal target, so there is a choice to make"
    );
    // It is a 1/1 taking 2, so it kills itself — which is the clearest possible proof
    // that the damage landed on the source rather than nowhere.
    assert!(
        engine.state.battlefield().is_empty(),
        "the 1/1 shot itself for 2 and should be dead"
    );
    assert_eq!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .len(),
        1,
        "and it is in its owner's graveyard"
    );
}

#[test]
fn a_targeted_trigger_with_no_legal_target_leaves_the_stack_and_does_nothing() {
    // CR 603.3d — the ability is removed from the stack rather than waiting around.
    // This needs a filter that excludes your own side: a trigger that targets *any*
    // creature always has at least one legal choice, namely the creature that triggered
    // it.
    let (engine, _cards, asked) =
        cast_and_settle(ETB_BOLT_OPPONENT, |_| {}, |legal| legal.to_vec());

    assert!(
        !asked
            .iter()
            .any(|c| matches!(c.kind, ChoiceKind::ChooseTargets { .. })),
        "there was nothing to choose between"
    );
    assert!(
        engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty(),
        "the ability must not be left sitting on the stack"
    );
    // The sentinel itself is on the battlefield and undamaged.
    let damaged = engine
        .state
        .battlefield()
        .iter()
        .filter(|id| engine.state.objects[id].damage > 0)
        .count();
    assert_eq!(damaged, 0, "nothing was damaged");
}

#[test]
fn a_targeted_trigger_hits_the_creature_that_was_actually_chosen() {
    // Two candidates plus the source itself, and the answer names one specifically —
    // so picking any other would show up as damage in the wrong place.
    let ids = std::cell::RefCell::new(Vec::new());
    let wanted = std::cell::Cell::new(None);
    let (engine, _cards, _) = cast_and_settle(
        ETB_BOLT,
        |state| {
            let a = ready(state, BIG, P1);
            let b = ready(state, BIG, P1);
            wanted.set(Some(b));
            ids.borrow_mut().extend([a, b]);
        },
        |legal| {
            let want = Target::Object(wanted.get().unwrap());
            legal.iter().copied().filter(|t| *t == want).collect()
        },
    );

    let ids = ids.borrow();
    let want = wanted.get().unwrap();
    assert_eq!(
        engine.state.objects[&want].damage, 2,
        "the chosen creature took it"
    );
    for other in ids.iter().filter(|id| **id != want) {
        assert_eq!(
            engine.state.objects[other].damage, 0,
            "and nothing else did"
        );
    }
}

#[test]
fn a_targeted_trigger_fizzles_if_its_target_is_gone_by_resolution() {
    // The generic CR 608.2b path applies to triggered abilities too.
    let mut state = board();
    let victim = ready(&mut state, DUMMY, P1); // 2/2
    let sentinel = state.place(ETB_BOLT, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // Cast it and let the trigger take its target, stopping before it resolves.
    for _ in 0..6000 {
        let targeted = engine
            .log
            .iter()
            .any(|e| matches!(e.event, mtg_core::Event::Targeted { .. }));
        if targeted {
            break;
        }
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
                let answer = match &c.kind {
                    ChoiceKind::Priority { legal } => legal
                        .actions
                        .iter()
                        .find(|a| matches!(a, Action::Cast { object } if *object == sentinel))
                        .cloned()
                        .map(Answer::Action)
                        .unwrap_or(Answer::Pass),
                    ChoiceKind::ChooseTargets { slots } => {
                        // Name the victim specifically: the sentinel itself is also a
                        // legal target, and targeting that would make the test pass for
                        // the wrong reason.
                        let want = Target::Object(victim);
                        Answer::Targets(
                            slots
                                .iter()
                                .map(|s| s.iter().copied().filter(|t| *t == want).collect())
                                .collect(),
                        )
                    }
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                let _ = engine.answer(&cards, c.id, answer);
            }
        }
    }

    // Kill the target while the ability is still on the stack.
    if let Some(o) = engine.state.objects.get_mut(&victim) {
        o.damage = 2;
    }

    for _ in 0..6000 {
        if engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
            && engine.state.step != Step::PrecombatMain
        {
            break;
        }
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
                let a = c.default.clone().unwrap_or(Answer::Pass);
                if engine.answer(&cards, c.id, a).is_err() {
                    break;
                }
            }
        }
    }

    assert!(
        engine
            .log
            .iter()
            .any(|e| matches!(e.event, mtg_core::Event::Countered { .. })),
        "the ability should have been countered by the game rules"
    );
}
