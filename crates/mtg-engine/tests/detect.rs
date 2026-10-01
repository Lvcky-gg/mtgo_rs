//! Trigger detection (CR 603.2): do triggered abilities actually fire?

mod common;

use common::*;
use mtg_core::{ObjectId, Step, Zone, ZoneRef};
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
    state
}

/// A permanent already in play and able to act.
fn ready(state: &mut GameState, card: mtg_core::CardId, who: mtg_core::PlayerId) -> ObjectId {
    let id = state.place(card, who, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    id
}

/// Drive until `stop`, answering priority by casting `want` if offered, else passing,
/// and attacking with `attackers` when asked.
fn drive(
    engine: &mut Engine,
    cards: &TestCards,
    want: Option<ObjectId>,
    attackers: &[ObjectId],
    budget: usize,
    mut stop: impl FnMut(&Engine) -> bool,
) {
    for _ in 0..budget {
        if stop(engine) {
            return;
        }
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => return,
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
                    ChoiceKind::DeclareAttackers { eligible, .. } => Answer::Objects(
                        attackers
                            .iter()
                            .copied()
                            .filter(|a| eligible.contains(a))
                            .collect(),
                    ),
                    ChoiceKind::DeclareBlockers { .. } => Answer::Blocks(Vec::new()),
                    ChoiceKind::OrderBlockers { blockers, .. } => {
                        Answer::Order((0..blockers.len()).collect())
                    }
                    ChoiceKind::OrderTriggers { triggers, .. } => Answer::Order(triggers.clone()),
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                if engine.answer(cards, c.id, answer).is_err() {
                    let fallback = c.default.clone().unwrap_or(Answer::Pass);
                    let _ = engine.answer(cards, c.id, fallback);
                }
            }
        }
    }
}

fn life(engine: &Engine, who: mtg_core::PlayerId) -> i32 {
    engine.state.player(who).life
}

// ---- the basic question ------------------------------------------------

#[test]
fn an_enters_trigger_fires_and_resolves() {
    // The source itself enters, so this is both detection and resolution.
    let mut state = board();
    let spell = state.place(ON_ENTER, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, Some(spell), &[], 4000, |e| {
        e.state.player(P0).life >= 23
    });

    assert_eq!(life(&engine, P0), 23, "20 + 3 from the enters trigger");
}

#[test]
fn an_attacks_trigger_fires() {
    let mut state = board();
    state.step = Step::BeginCombat;
    let attacker = ready(&mut state, ON_ATTACK, P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, None, &[attacker], 4000, |e| {
        e.state.step == Step::PostcombatMain
    });

    assert_eq!(life(&engine, P0), 21, "20 + 1 from the attacks trigger");
}

#[test]
fn a_dies_trigger_fires_even_though_the_source_has_left_the_battlefield() {
    // CR 603.10: the creature is in the graveyard by the time anything can look, so
    // this only works because last-known information is kept.
    let mut state = board();
    let doomed = ready(&mut state, ON_DEATH, P0);
    state.objects.get_mut(&doomed).unwrap().damage = 2; // lethal for a 2/2
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, None, &[], 2000, |e| {
        e.state.player(P0).life >= 22
    });

    assert!(!engine.state.objects.contains_key(&doomed), "it died");
    assert_eq!(life(&engine, P0), 22, "20 + 2 from the dies trigger");
}

#[test]
fn an_upkeep_trigger_fires_at_the_beginning_of_upkeep() {
    let mut state = board();
    state.step = Step::Untap;
    ready(&mut state, ON_UPKEEP, P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, None, &[], 2000, |e| {
        e.state.step == Step::Draw
    });
    assert_eq!(life(&engine, P0), 21);
}

#[test]
fn an_upkeep_trigger_does_not_fire_on_the_opponents_upkeep() {
    // "At the beginning of *your* upkeep".
    let mut state = board();
    state.step = Step::Untap;
    state.active_player = P1;
    ready(&mut state, ON_UPKEEP, P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, None, &[], 2000, |e| {
        e.state.step == Step::Draw
    });
    assert_eq!(life(&engine, P0), 20, "not this player's upkeep");
}

// ---- filters -----------------------------------------------------------

#[test]
fn a_controlled_by_you_filter_ignores_an_opponents_creature() {
    let mut state = board();
    ready(&mut state, ONCE_PER_TURN, P0); // "a creature you control enters"
    // The opponent's creature enters.
    let theirs = state.place(DUMMY, P1, ZoneRef::of(Zone::Hand, P1));
    state.active_player = P1;
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, Some(theirs), &[], 4000, |e| {
        e.state.step == Step::BeginCombat
    });

    assert_eq!(life(&engine, P0), 20, "the filter says 'you control'");
}

#[test]
fn an_opponent_filter_fires_for_an_opponents_creature() {
    let mut state = board();
    ready(&mut state, ON_OPPONENT_ENTER, P0);
    let theirs = state.place(DUMMY, P1, ZoneRef::of(Zone::Hand, P1));
    state.active_player = P1;
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, Some(theirs), &[], 4000, |e| {
        e.state.player(P0).life >= 21
    });

    assert_eq!(life(&engine, P0), 21);
}

// ---- intervening if (CR 603.4) -----------------------------------------

#[test]
fn an_intervening_if_that_fails_stops_the_trigger_reaching_the_stack() {
    let mut state = board();
    state.step = Step::Untap;
    ready(&mut state, GATED, P0); // needs three creatures you control
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, None, &[], 2000, |e| {
        e.state.step == Step::Draw
    });
    assert_eq!(life(&engine, P0), 20, "one creature is not three");
}

#[test]
fn an_intervening_if_that_holds_lets_the_trigger_through() {
    let mut state = board();
    state.step = Step::Untap;
    ready(&mut state, GATED, P0);
    ready(&mut state, DUMMY, P0);
    ready(&mut state, DUMMY, P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, None, &[], 2000, |e| {
        e.state.player(P0).life >= 25
    });
    assert_eq!(life(&engine, P0), 25, "three creatures, so 20 + 5");
}

// ---- limits ------------------------------------------------------------

#[test]
fn a_once_each_turn_trigger_fires_only_once() {
    let mut state = board();
    ready(&mut state, ONCE_PER_TURN, P0);
    let a = state.place(DUMMY, P0, ZoneRef::of(Zone::Hand, P0));
    let b = state.place(DUMMY, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // Cast both in the same turn.
    drive(&mut engine, &cards, Some(a), &[], 3000, |e| {
        e.state.battlefield().len() >= 2
    });
    drive(&mut engine, &cards, Some(b), &[], 3000, |e| {
        e.state.battlefield().len() >= 3
    });
    drive(&mut engine, &cards, None, &[], 3000, |e| {
        e.state.step == Step::End
    });

    assert_eq!(
        engine.state.battlefield().len(),
        3,
        "both creatures arrived"
    );
    assert_eq!(
        life(&engine, P0),
        21,
        "but the trigger was capped at once per turn"
    );
}

// ---- bindings ----------------------------------------------------------

#[test]
fn the_event_subject_binding_refers_to_the_thing_that_entered() {
    // "Whenever a creature you control enters, tap it" — `it` is the newcomer, not
    // the ability's source. Getting this wrong taps the wrong permanent, which is why
    // bindings are captured at detection time rather than looked up on resolution.
    let mut state = board();
    let sigil = ready(&mut state, TAP_THE_NEWCOMER, P0);
    let newcomer = state.place(DUMMY, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, Some(newcomer), &[], 4000, |e| {
        e.state.battlefield().iter().any(|id| {
            e.state
                .objects
                .get(id)
                .is_some_and(|o| o.tapped && o.card == DUMMY)
        })
    });

    let tapped_dummy = engine
        .state
        .battlefield()
        .iter()
        .filter(|id| {
            engine
                .state
                .objects
                .get(id)
                .is_some_and(|o| o.card == DUMMY && o.tapped)
        })
        .count();
    assert_eq!(tapped_dummy, 1, "the creature that entered got tapped");
    assert!(
        !engine.state.objects[&sigil].tapped,
        "the ability's source must not be the one tapped"
    );
}

// ---- triggers use the stack -------------------------------------------

#[test]
fn a_fired_trigger_goes_on_the_stack_rather_than_happening_immediately() {
    let mut state = board();
    state.step = Step::Untap;
    ready(&mut state, ON_UPKEEP, P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    // Stop the moment something is on the stack, before anyone could have passed.
    let mut saw_on_stack = false;
    for _ in 0..500 {
        if !engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
        {
            saw_on_stack = true;
            break;
        }
        match engine.advance(&cards) {
            Progress::NeedsChoice(c) => {
                let _ = engine.answer(&cards, c.id, c.default.clone().unwrap_or(Answer::Pass));
            }
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
        }
    }

    assert!(saw_on_stack, "the trigger should have used the stack");
    assert_eq!(
        life(&engine, P0),
        20,
        "and not taken effect before resolving"
    );
}

// ---- detection is a function of the log --------------------------------

#[test]
fn detection_is_deterministic_across_a_replay() {
    // The same log must detect the same triggers, which is what makes replay and
    // reconnect trustworthy.
    let build = || {
        let mut state = board();
        state.step = Step::Untap;
        ready(&mut state, ON_UPKEEP, P0);
        Engine::new(state)
    };
    let cards = TestCards::default();

    let mut a = build();
    drive(&mut a, &cards, None, &[], 2000, |e| {
        e.state.step == Step::Draw
    });
    let mut b = build();
    drive(&mut b, &cards, None, &[], 2000, |e| {
        e.state.step == Step::Draw
    });

    assert_eq!(life(&a, P0), life(&b, P0));
    assert_eq!(a.log.len(), b.log.len(), "identical logs");
}

#[test]
fn casting_a_creature_does_not_resolve_its_triggered_ability() {
    // Regression: a spell resolves only its `SpellEffect`. A permanent spell has
    // none — it just becomes a permanent. Matching "the first ability" made casting a
    // creature with an attack trigger fire that trigger's effect on resolution.
    let mut state = board();
    let banner = state.place(ON_ATTACK, P0, ZoneRef::of(Zone::Hand, P0));
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, Some(banner), &[], 4000, |e| {
        e.state.step == Step::BeginCombat
    });

    assert_eq!(
        engine.state.battlefield().len(),
        1,
        "it resolved onto the battlefield"
    );
    assert_eq!(
        life(&engine, P0),
        20,
        "an attacks trigger must not fire just because the creature was cast"
    );
}

#[test]
fn a_creature_cast_this_turn_still_triggers_when_it_attacks_later() {
    // The other half: the trigger is intact, it just fires at the right time.
    let mut state = board();
    state.step = Step::BeginCombat;
    let attacker = ready(&mut state, ON_ATTACK, P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, None, &[attacker], 4000, |e| {
        e.state.step == Step::PostcombatMain
    });
    assert_eq!(life(&engine, P0), 21);
}

// ---- "this" means this ---------------------------------------------------

#[test]
fn a_self_attack_trigger_ignores_other_attackers() {
    // One banner and one plain creature attack: only the banner's own attack counts.
    let mut state = board();
    state.step = Step::BeginCombat;
    let banner = ready(&mut state, ON_ATTACK, P0);
    let other = ready(&mut state, DUMMY, P0);
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, None, &[banner, other], 4000, |e| {
        e.state.step == Step::PostcombatMain
    });
    assert_eq!(life(&engine, P0), 21, "once, for the banner itself");
}

#[test]
fn a_self_dies_trigger_ignores_other_deaths() {
    let mut state = board();
    ready(&mut state, ON_DEATH, P0);
    let doomed = ready(&mut state, DUMMY, P0);
    state.objects.get_mut(&doomed).unwrap().damage = 99;
    let mut engine = Engine::new(state);
    let cards = TestCards::default();

    drive(&mut engine, &cards, None, &[], 400, |e| {
        !e.state.objects.contains_key(&doomed)
    });
    drive(&mut engine, &cards, None, &[], 400, |_| false);
    assert_eq!(
        life(&engine, P0),
        20,
        "another creature dying is not this one dying"
    );
}
