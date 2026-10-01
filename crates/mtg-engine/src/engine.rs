//! The driver.
//!
//! [`Engine`] owns the state and the log and steps the game forward. The contract
//! is a loop:
//!
//! ```text
//! loop {
//!     match engine.advance(cards) {
//!         Progress::Continue      => continue,
//!         Progress::NeedsChoice(c) => engine.answer(c.id, policy_or_human(c))?,
//!         Progress::GameOver{..}  => break,
//!     }
//! }
//! ```
//!
//! `advance` never blocks and never asks: it either makes progress or hands back a
//! question. That shape is what lets the same engine serve a GUI, a scripted test
//! and a network host without any of them being privileged — and it is why a
//! reconnecting peer needs no special path, since a `Choice` that was already
//! answered is simply not re-emitted.
//!
//! # The settle phase is the interesting part
//!
//! Between every action and the next grant of priority sits [`Phase::Settle`],
//! which implements CR 117.5:
//!
//! 1. Check state-based actions. If any apply, perform **all** of them at once and
//!    start over — performing them can make more apply.
//! 2. Only once no state-based action applies, put waiting triggered abilities on
//!    the stack.
//! 3. Repeat until a pass does neither.
//!
//! The order matters and is a common place to go wrong. Doing it this way means a
//! permanent that dies to a state-based action, and the trigger that killed it,
//! resolve correctly without either knowing about the other.

use std::collections::VecDeque;

use mtg_core::{
    Cause, Event, LossReason, ObjectId, PlayerId, StampedEvent, Step, Target, Zone, ZoneRef,
};

use crate::{
    actions::{Action, LegalActions},
    apply,
    choice::{Answer, Choice, ChoiceKind},
    layers::{CharacteristicsCache, PrintedCards},
    resolve::{self, ResolveCtx},
    sba::{self, Sba},
    state::GameState,
    triggers::{self, Ordering, PendingTrigger},
    turn,
};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Illegal {
    /// An answer arrived for a choice that is not outstanding. Benign — this is
    /// what a duplicate answer after a reconnect looks like.
    NoSuchChoice(u64),
    /// The answer was the wrong shape for the question.
    WrongAnswerKind,
    /// The action is not in the legal set.
    NotLegal(Action),
    /// An undo was asked for when there is nothing this player may take back.
    CannotUndo,
}

#[derive(Clone, Debug)]
pub enum Progress {
    /// Advanced. Call again.
    Continue,
    /// A player must answer before anything else can happen.
    NeedsChoice(Choice),
    GameOver {
        winners: Vec<PlayerId>,
    },
}

/// A turn-based action that has started and is waiting on answers.
///
/// This is the piece that makes combat possible. Most turn-based actions run to
/// completion the moment their step begins — untapping, drawing for turn — but
/// declaring attackers and blockers require player decisions *inside* the action,
/// before any player receives priority. So the action has to be able to stop, hand
/// back a question, and pick up where it left off.
///
/// Each variant is a resume point. Holding a queue rather than an index means a
/// multi-step sub-flow (ordering blockers for three separate attackers) is one
/// state, not three.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Suspended {
    /// CR 502.3 — the active player chooses which "may choose not to untap" permanents
    /// stay tapped.
    Untapping,
    /// CR 508.1 — waiting for the active player to declare attackers.
    Attackers,
    /// CR 509.1 — waiting for the defending player to declare blockers.
    Blockers,
    /// CR 509.2 — the attacking player orders the blockers of each blocked attacker.
    /// One attacker at a time, from the front of the queue.
    OrderingBlockers { queue: Vec<ObjectId> },
    /// CR 510.1c — damage assignment for attackers where it is not forced.
    AssigningDamage {
        queue: Vec<ObjectId>,
        first_strike: bool,
        /// Assignments settled so far, applied together once the queue empties, so
        /// that combat damage stays simultaneous (CR 510.2).
        settled: Vec<crate::combat::Assignment>,
    },
    /// CR 514.1 — the active player is choosing what to discard down to hand size.
    Discarding,
}

/// An announcement in progress, waiting for targets (CR 601.2).
///
/// Casting is a sequence of steps, and choosing targets is one of them — so, like
/// resolution, it cannot finish in a single call. Unlike resolution it needs no replay:
/// choosing a target changes nothing, so answers simply accumulate.
///
/// The snapshot is for the other half of CR 601.2: if the announcement cannot be
/// completed legally, *the whole thing is rewound* and the player is as they were. That
/// should not happen, because `legal_actions` only offers a spell whose targets can be
/// chosen, but rewinding is the correct failure rather than leaving a half-announced
/// spell on the stack.
struct Announcing {
    /// The stack object being announced.
    object: ObjectId,
    controller: PlayerId,
    specs: Vec<mtg_ir::selector::TargetSpec>,
    /// Which slot is being filled.
    slot: usize,
    /// Targets chosen so far, one per slot.
    chosen: Vec<Target>,
    /// Positions in `chosen` holding a placeholder for a slot left empty — an "up to one"
    /// slot with nothing chosen. Keeping a placeholder keeps later slots at their own
    /// index, so `Target { index }` still names the right one.
    empty: Vec<u8>,
    /// Whether this is an ability rather than a spell, which changes what happens on
    /// completion (no cost is paid for a triggered ability).
    pay_cost: bool,
    snapshot: Box<GameState>,
    log_len: usize,
    /// CR 601.2b — a modal spell's modes are chosen first: the labels and how many.
    modal: Option<(Vec<Box<str>>, u8, u8)>,
    modes: Option<Vec<u8>>,
    /// CR 601.2b — then the value of `{X}`, up to the most that could be paid.
    max_x: Option<u32>,
    x: Option<u32>,
    /// CR 601.2b — whether to pay a kicker cost, asked only when it could be paid.
    kicker: bool,
    kicked: Option<bool>,
    /// CR 601.2h — cost parts that are a choice ("sacrifice a creature"), in order, and
    /// what has been chosen for each so far.
    cost_parts: Vec<mtg_ir::AdditionalCost>,
    cost_chosen: Vec<Vec<ObjectId>>,
}

impl Announcing {
    /// What the announcement is waiting for next.
    fn awaiting(&self) -> Awaiting {
        if self.kicker && self.kicked.is_none() {
            Awaiting::Kicker
        } else if self.modal.is_some() && self.modes.is_none() {
            Awaiting::Modes
        } else if self.max_x.is_some() && self.x.is_none() {
            Awaiting::X
        } else if self.slot < self.specs.len() {
            Awaiting::Targets
        } else if self.cost_chosen.len() < self.cost_parts.len() {
            Awaiting::Cost
        } else {
            Awaiting::Targets
        }
    }

    /// Leave the current slot empty, keeping later slots at their indices.
    fn skip_slot(&mut self) {
        self.empty.push(self.chosen.len() as u8);
        // The placeholder points at the object being announced, which is never a legal
        // target of itself; it is marked illegal on resolution and so selects nothing.
        self.chosen.push(Target::Object(self.object));
        self.slot += 1;
    }
}

/// What an announcement settled: targets one per slot (with placeholders for empty
/// slots), the chosen modes, and X.
#[derive(Clone, Debug, Default)]
struct Announced {
    targets: Vec<Target>,
    empty: Vec<u8>,
    modes: Vec<u8>,
    x: u32,
    kicked: bool,
    /// Objects chosen to pay cost parts: sacrificed or discarded (moved), or tapped.
    paid_with: Vec<ObjectId>,
    tapped_for: Vec<ObjectId>,
    /// Unblocked attackers returned to hand to pay for ninjutsu.
    returned: Vec<ObjectId>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Awaiting {
    Cost,
    Kicker,
    Modes,
    X,
    Targets,
}

/// The printed card of an object, or a placeholder when it is gone.
fn obj_card(state: &GameState, id: ObjectId) -> mtg_core::CardId {
    state
        .objects
        .get(&id)
        .map(|o| o.card)
        .unwrap_or(mtg_core::CardId(u32::MAX))
}

/// A resolution in progress, waiting on an answer.
///
/// Resolution is a recursive tree walk that applies events as it goes, so it cannot
/// suspend the way a turn-based action can — there is no small resume point to store.
/// It is made **restartable** instead: the state before the resolution is kept, and
/// when a choice is needed the state is rolled back, the player is asked, the answer is
/// appended, and the whole resolution runs again.
///
/// That is sound because resolution is deterministic given its starting state and the
/// answers, so the Nth choice on a re-run is necessarily the same Nth choice. It costs
/// one replay per choice and effects have only a handful. It also makes resolution
/// restartable in general, which is exactly what taking a choice back requires.
struct Resolving {
    /// The stack object being resolved. If the top of the stack is not this, the
    /// in-flight resolution is stale and is discarded.
    object: ObjectId,
    /// State as it was before the resolution began.
    snapshot: Box<GameState>,
    /// Log length before the resolution began, for truncation on rollback.
    log_len: usize,
    /// Answers gathered so far, in the order asked.
    answers: Vec<Answer>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    BeginStep,
    /// A spell or ability is being announced and needs its targets.
    Announcing,
    TurnBasedAction,
    Settle,
    Priority,
    ResolveTop,
    EndStep,
    Over,
}

pub struct Engine {
    pub state: GameState,
    pub log: Vec<StampedEvent>,
    cache: CharacteristicsCache,
    phase: Phase,
    pending: Option<Choice>,
    next_choice_id: u64,
    /// Trigger batches whose controller still has to order them.
    to_order: VecDeque<(PlayerId, Vec<PendingTrigger>)>,
    /// Triggers whose order is settled, waiting to go on the stack.
    ///
    /// Placed **one at a time** rather than as a batch, because a triggered ability
    /// chooses its targets as it is put on the stack (CR 603.3d) — so placing one can
    /// need a question, and the rest of the batch has to wait behind it.
    to_place: VecDeque<(PlayerId, PendingTrigger)>,
    /// Whether this step's turn-based action has run.
    tba_done: bool,
    /// A turn-based action mid-flight, waiting on answers.
    suspended: Option<Suspended>,
    /// An announcement mid-flight, waiting for targets.
    announcing: Option<Announcing>,
    /// The id of the outstanding choice that belongs to `announcing`.
    announcement_choice: Option<u64>,
    /// A resolution mid-flight, waiting on answers.
    resolving: Option<Resolving>,
    /// The id of the outstanding choice that belongs to `resolving`, so an answer can
    /// be routed to the resolution rather than mistaken for an unrelated question.
    resolution_choice: Option<u64>,
    /// Points a player can undo back to, newest last. See [`Engine::can_undo`].
    undo: Vec<Checkpoint>,
    /// The id of the outstanding mulligan question, so its answer is routed here.
    pregame_choice: Option<u64>,
    /// A land just played that asks for life as it enters (CR 614.12): the question's id
    /// and the land.
    entering_choice: Option<(u64, ObjectId)>,
    /// Permanents the active player chose to leave tapped this untap step (CR 502.3).
    keep_tapped: Vec<ObjectId>,
    /// Whether the cards with a graveyard-to-exile replacement have been recorded.
    redirects_noted: bool,
}

/// The engine as it was when a player took a priority action.
///
/// Only taken at priority with nothing in flight — no announcement, resolution,
/// suspended turn-based action or trigger queue — so these fields are the whole of
/// the engine's state at that moment and restoring them is a complete rewind.
struct Checkpoint {
    who: PlayerId,
    state: GameState,
    log_len: usize,
    phase: Phase,
    tba_done: bool,
    /// The priority question the action answered, asked again on undo.
    question: Choice,
}

/// How many actions back a player can undo. A turn rarely has more; the cap only
/// bounds memory.
const UNDO_DEPTH: usize = 64;

impl Engine {
    pub fn new(state: GameState) -> Self {
        Self {
            state,
            log: Vec::new(),
            cache: CharacteristicsCache::default(),
            phase: Phase::BeginStep,
            pending: None,
            next_choice_id: 1,
            to_order: VecDeque::new(),
            to_place: VecDeque::new(),
            tba_done: false,
            suspended: None,
            announcing: None,
            announcement_choice: None,
            resolving: None,
            resolution_choice: None,
            undo: Vec::new(),
            pregame_choice: None,
            entering_choice: None,
            keep_tapped: Vec::new(),
            redirects_noted: false,
        }
    }

    /// Step the game forward by the smallest meaningful amount.
    pub fn advance(&mut self, cards: &dyn PrintedCards) -> Progress {
        self.note_graveyard_redirects(cards);
        // An outstanding question blocks everything. Re-emitting the same choice is
        // deliberate: it makes `advance` idempotent, which is what a reconnecting
        // client needs.
        if let Some(c) = &self.pending {
            return Progress::NeedsChoice(c.clone());
        }

        if let Some(winners) = self.game_over() {
            self.undo.clear();
            self.phase = Phase::Over;
            return Progress::GameOver { winners };
        }

        // Opening hands are settled before the first turn begins (CR 103.5).
        if self.state.pregame.is_some() {
            return self.pregame_question();
        }

        match self.phase {
            Phase::Over => Progress::GameOver {
                winners: self.winners(),
            },

            Phase::BeginStep => {
                let (turn, active, step) =
                    (self.state.turn, self.state.active_player, self.state.step);
                apply::apply(
                    &mut self.state,
                    Cause::TurnStructure,
                    Event::StepBegan { turn, active, step },
                    &mut self.log,
                );
                self.cache.invalidate();
                self.tba_done = false;
                self.phase = Phase::TurnBasedAction;
                Progress::Continue
            }

            Phase::TurnBasedAction => {
                // A turn-based action may need answers part-way through, so it can
                // hand one back instead of completing. `suspended` is the resume
                // point.
                if let Some(choice) = self.drive_turn_based_action(cards) {
                    return Progress::NeedsChoice(choice);
                }
                self.phase = Phase::Settle;
                Progress::Continue
            }

            Phase::Announcing => {
                if let Some(choice) = self.drive_announcement(cards) {
                    return Progress::NeedsChoice(choice);
                }
                self.phase = Phase::Settle;
                Progress::Continue
            }

            Phase::Settle => self.settle(cards),

            Phase::Priority => self.give_priority(cards),

            Phase::ResolveTop => {
                // Resolution can need answers part-way through, so it may hand one back
                // rather than completing. The phase stays put until it finishes.
                if let Some(choice) = self.try_resolve_top(cards) {
                    return Progress::NeedsChoice(choice);
                }
                self.phase = Phase::Settle;
                Progress::Continue
            }

            Phase::EndStep => {
                let (turn, active, step) =
                    (self.state.turn, self.state.active_player, self.state.step);
                apply::apply(
                    &mut self.state,
                    Cause::TurnStructure,
                    Event::StepEnded { turn, active, step },
                    &mut self.log,
                );
                self.advance_step(cards);
                Progress::Continue
            }
        }
    }

    /// Answer an outstanding choice.
    ///
    /// Takes the card database because answering can require it: paying for a spell
    /// means reading mana abilities off the permanents that pay for it. The
    /// signature says so rather than the engine holding a reference and pretending
    /// otherwise.
    pub fn answer(
        &mut self,
        cards: &dyn PrintedCards,
        id: u64,
        answer: Answer,
    ) -> Result<(), Illegal> {
        let Some(choice) = self.pending.take() else {
            return Err(Illegal::NoSuchChoice(id));
        };
        if choice.id != id {
            // Put it back: an answer to a stale choice must not consume the live one.
            self.pending = Some(choice);
            return Err(Illegal::NoSuchChoice(id));
        }

        if matches!(answer, Answer::Undo) {
            return self.undo_last(choice);
        }
        if self.pregame_choice == Some(id) {
            return self.pregame_answer(choice, answer);
        }
        if let Some((cid, land)) = self.entering_choice
            && cid == id
        {
            if !matches!(answer, Answer::Bool(_) | Answer::Modes(_)) {
                self.pending = Some(choice);
                return Err(Illegal::WrongAnswerKind);
            }
            self.entering_choice = None;
            resolve::entered(
                &mut self.state,
                cards,
                &mut self.log,
                land,
                Cause::PlayerAction(choice.who),
                Some(answer),
            );
            self.cache.invalidate();
            return Ok(());
        }

        // A target answer belongs to the announcement in flight.
        if self.announcement_choice == Some(id) {
            // A wrongly-shaped answer is rejected rather than ignored. Ignoring it left
            // the slot unfilled and the same question asked again forever — a livelock
            // rather than an error, which is the worse of the two failures.
            let awaiting = self.announcing.as_ref().map(Announcing::awaiting);
            let shaped = matches!(
                (awaiting, &answer),
                (Some(Awaiting::Targets), Answer::Targets(_))
                    | (Some(Awaiting::Cost), Answer::Objects(_))
                    | (Some(Awaiting::Kicker), Answer::Bool(_))
                    | (Some(Awaiting::X), Answer::Number(_))
                    | (Some(Awaiting::Modes), Answer::Modes(_))
            );
            if !shaped {
                self.pending = Some(choice);
                return Err(Illegal::WrongAnswerKind);
            }
            if let (Some(a), Answer::Modes(picked)) = (self.announcing.as_mut(), &answer) {
                // The required number of distinct modes (CR 700.2): exactly `count`, or
                // between the fewest and `count` ("choose one or both").
                let (labels, count, min) = a.modal.clone().unwrap_or_default();
                let mut distinct = picked.clone();
                distinct.sort_unstable();
                distinct.dedup();
                if distinct.len() != picked.len()
                    || picked.len() > count as usize
                    || picked.len() < min as usize
                    || picked.iter().any(|m| *m as usize >= labels.len())
                {
                    self.pending = Some(choice);
                    return Err(Illegal::WrongAnswerKind);
                }
                a.modes = Some(distinct);
                self.announcement_choice = None;
                return Ok(());
            }
            if let (Some(a), Answer::Objects(picked)) = (self.announcing.as_mut(), &answer) {
                let ChoiceKind::ChooseObjects { from, min, max } = &choice.kind else {
                    self.pending = Some(choice);
                    return Err(Illegal::WrongAnswerKind);
                };
                let mut distinct = picked.clone();
                distinct.sort_unstable();
                distinct.dedup();
                // Crew needs enough power; everything else an exact count.
                let enough = match a.cost_parts.get(a.cost_chosen.len()) {
                    Some(mtg_ir::AdditionalCost::TapCreaturesWithPower {
                        power: mtg_ir::Value::Fixed(n),
                    }) => crate::cost::total_power(&self.state, cards, picked) >= *n,
                    _ => picked.len() == *min as usize,
                };
                if distinct.len() != picked.len()
                    || !enough
                    || picked.len() < *min as usize
                    || picked.len() > *max as usize
                    || !picked.iter().all(|o| from.contains(o))
                {
                    self.pending = Some(choice);
                    return Err(Illegal::WrongAnswerKind);
                }
                a.cost_chosen.push(picked.clone());
                self.announcement_choice = None;
                return Ok(());
            }
            if let (Some(a), Answer::Bool(b)) = (self.announcing.as_mut(), &answer) {
                a.kicked = Some(*b);
                // Entwined: every mode (CR 702.42b).
                let entwine = self
                    .state
                    .objects
                    .get(&a.object)
                    .and_then(|o| cards.face(o.card, o.face))
                    .is_some_and(crate::cost::is_entwine);
                if *b
                    && entwine
                    && let Some((labels, _, _)) = &a.modal
                {
                    a.modes = Some((0..labels.len() as u8).collect());
                }
                self.announcement_choice = None;
                return Ok(());
            }
            if let (Some(a), Answer::Number(n)) = (self.announcing.as_mut(), &answer) {
                if *n > a.max_x.unwrap_or(0) {
                    self.pending = Some(choice);
                    return Err(Illegal::WrongAnswerKind);
                }
                a.x = Some(*n);
                self.announcement_choice = None;
                return Ok(());
            }
            self.announcement_choice = None;
            if let (Some(a), Answer::Targets(slots)) = (self.announcing.as_mut(), &answer) {
                let spec = a.specs.get(a.slot).cloned();
                let picked: Vec<Target> = slots.iter().flatten().copied().collect();
                // Only legal targets count; an answer that under-delivers on a required
                // slot is topped up from the legal set, so a malformed answer cannot
                // sneak a spell onto the stack with no targets.
                if let Some(spec) = spec {
                    let legal = crate::targeting::legal_targets(
                        &self.state,
                        cards,
                        &spec,
                        a.object,
                        a.controller,
                        &a.chosen,
                    );
                    let need = crate::targeting::required(
                        &spec,
                        &self.state,
                        cards,
                        a.object,
                        a.controller,
                    );
                    let mut keep: Vec<Target> =
                        picked.into_iter().filter(|t| legal.contains(t)).collect();
                    keep.dedup();
                    for t in &legal {
                        if keep.len() as u32 >= need {
                            break;
                        }
                        if !keep.contains(t) {
                            keep.push(*t);
                        }
                    }
                    if keep.is_empty() {
                        // "Up to one" and none chosen: the slot stays empty.
                        a.skip_slot();
                    } else {
                        a.chosen.extend(keep);
                        a.slot += 1;
                    }
                } else {
                    a.slot += 1;
                }
            }
            return Ok(());
        }

        // An answer that belongs to an in-flight resolution is appended to its answer
        // list; the resolution then runs again from the start with it in hand.
        if self.resolution_choice == Some(id) {
            self.resolution_choice = None;
            if let Some(r) = self.resolving.as_mut() {
                r.answers.push(answer);
            }
            return Ok(());
        }

        match (&choice.kind, answer) {
            (ChoiceKind::Priority { legal }, Answer::Pass) => {
                let _ = legal;
                self.record_pass(choice.who);
                Ok(())
            }
            (ChoiceKind::Priority { legal }, Answer::Action(a)) => {
                if !legal.actions.contains(&a) && !legal.mana_abilities.contains(&a) {
                    self.pending = Some(choice);
                    return Err(Illegal::NotLegal(a));
                }
                self.checkpoint(&choice);
                self.take_action(cards, choice.who, a);
                Ok(())
            }
            (ChoiceKind::ChooseObjects { from, .. }, Answer::Objects(chosen))
                if matches!(self.suspended, Some(Suspended::Untapping)) =>
            {
                if !chosen.iter().all(|o| from.contains(o)) {
                    self.pending = Some(choice);
                    return Err(Illegal::WrongAnswerKind);
                }
                self.keep_tapped = chosen;
                self.suspended = None;
                self.perform_turn_based_action(cards, turn::TurnBasedAction::UntapAll);
                self.keep_tapped.clear();
                Ok(())
            }
            (ChoiceKind::ChooseObjects { from, min, .. }, Answer::Objects(chosen))
                if matches!(self.suspended, Some(Suspended::Discarding)) =>
            {
                let mut distinct = chosen.clone();
                distinct.sort_unstable();
                distinct.dedup();
                if distinct.len() != chosen.len()
                    || chosen.len() != *min as usize
                    || !chosen.iter().all(|o| from.contains(o))
                {
                    self.pending = Some(choice);
                    return Err(Illegal::WrongAnswerKind);
                }
                self.discarded_to_hand_size(cards, choice.who, &chosen);
                Ok(())
            }
            (ChoiceKind::DeclareAttackers { defenders, .. }, Answer::Objects(attackers)) => {
                let destination = defenders.first().copied();
                let declaration: Vec<_> = attackers
                    .into_iter()
                    .filter_map(|a| destination.map(|d| (a, d)))
                    .collect();
                if !self.declared_attackers(cards, &declaration) {
                    self.pending = Some(choice);
                    return Err(Illegal::WrongAnswerKind);
                }
                Ok(())
            }
            (ChoiceKind::DeclareAttackers { .. }, Answer::Attackers(attackers)) => {
                if !self.declared_attackers(cards, &attackers) {
                    self.pending = Some(choice);
                    return Err(Illegal::WrongAnswerKind);
                }
                Ok(())
            }

            (ChoiceKind::DeclareBlockers { .. }, Answer::Blocks(pairs)) => {
                // An illegal declaration is rejected and the question re-asked, rather
                // than being quietly repaired. Silently dropping an illegal blocker
                // would change the player's decision without telling them.
                match self.declared_blockers(cards, choice.who, &pairs) {
                    Ok(()) => Ok(()),
                    Err(_) => {
                        self.pending = Some(choice);
                        Err(Illegal::WrongAnswerKind)
                    }
                }
            }

            (ChoiceKind::OrderBlockers { attacker, .. }, Answer::Order(order)) => {
                let attacker = *attacker;
                if let Some(blockers) = self.state.combat.blocks.get(&attacker).cloned() {
                    let mut reordered: Vec<ObjectId> = order
                        .iter()
                        .filter_map(|i| blockers.get(*i).copied())
                        .collect();
                    // Anything the answer left out keeps its previous position, so a
                    // short answer cannot drop a blocker out of combat.
                    for b in &blockers {
                        if !reordered.contains(b) {
                            reordered.push(*b);
                        }
                    }
                    self.state.combat.blocks.insert(attacker, reordered);
                    self.state.bump();
                }
                // Move past this attacker and carry on with the queue.
                if let Some(Suspended::OrderingBlockers { queue }) = self.suspended.clone() {
                    let rest: Vec<ObjectId> =
                        queue.into_iter().filter(|a| *a != attacker).collect();
                    self.suspended = Some(Suspended::OrderingBlockers { queue: rest });
                }
                Ok(())
            }

            (
                ChoiceKind::AssignCombatDamage {
                    attacker,
                    among,
                    total,
                },
                Answer::DamageAssignment(rows),
            ) => {
                let attacker = *attacker;
                let deathtouch = self.has_deathtouch(cards, attacker);
                // The wire answer contains only object targets. Accepting the default must
                // retain the canonical assignment's excess trample damage to a player.
                let canonical = if matches!(&choice.default,
                    Some(Answer::DamageAssignment(default)) if *default == rows)
                {
                    self.state
                        .combat
                        .attackers
                        .get(&attacker)
                        .and_then(|defender| {
                            crate::combat::assign_attacker_damage(
                                &self.state,
                                cards,
                                attacker,
                                *defender,
                            )
                        })
                } else {
                    None
                };
                let mut trample_excess = None;
                if canonical.is_none() {
                    let mut seen = std::collections::BTreeSet::new();
                    let assigned: u64 = rows.iter().map(|(_, n)| u64::from(*n)).sum();
                    let valid_rows = rows
                        .iter()
                        .all(|(object, _)| among.contains(object) && seen.insert(*object));
                    let mut nonlethal = false;
                    let valid_order = among.iter().all(|blocker| {
                        let amount = rows
                            .iter()
                            .find(|(object, _)| object == blocker)
                            .map_or(0, |(_, n)| *n);
                        if nonlethal && amount > 0 {
                            return false;
                        }
                        if amount
                            < crate::combat::lethal_damage(&self.state, cards, *blocker, deathtouch)
                        {
                            nonlethal = true;
                        }
                        true
                    });
                    let excess = u64::from(*total).saturating_sub(assigned);
                    if excess > 0
                        && !nonlethal
                        && self.has_kw(cards, attacker, mtg_ir::ability::Keyword::Trample)
                    {
                        trample_excess = self
                            .state
                            .combat
                            .attackers
                            .get(&attacker)
                            .map(|defender| (*defender, excess as u32));
                    }
                    if !valid_rows
                        || assigned > u64::from(*total)
                        || !valid_order
                        || (excess > 0 && trample_excess.is_none())
                    {
                        self.pending = Some(choice);
                        return Err(Illegal::WrongAnswerKind);
                    }
                }
                let assignment = canonical.unwrap_or_else(|| crate::combat::Assignment {
                    source: attacker,
                    deathtouch,
                    to: rows
                        .into_iter()
                        .map(|(o, n)| (Target::Object(o), n))
                        .chain(trample_excess)
                        .collect(),
                });
                if let Some(Suspended::AssigningDamage {
                    queue,
                    first_strike,
                    mut settled,
                }) = self.suspended.clone()
                {
                    settled.push(assignment);
                    let rest: Vec<ObjectId> =
                        queue.into_iter().filter(|a| *a != attacker).collect();
                    self.suspended = Some(Suspended::AssigningDamage {
                        queue: rest,
                        first_strike,
                        settled,
                    });
                }
                Ok(())
            }

            (ChoiceKind::KeepOneLegend { candidates, .. }, Answer::Objects(kept)) => {
                // Whichever one was kept stays; the rest go to their owners'
                // graveyards, all at once, as one state-based action (CR 704.3).
                let keep = kept
                    .first()
                    .copied()
                    .or_else(|| candidates.first().copied());
                let doomed: Vec<ObjectId> = candidates
                    .iter()
                    .copied()
                    .filter(|c| Some(*c) != keep)
                    .collect();
                let events = doomed
                    .into_iter()
                    .filter_map(|object| {
                        let o = self.state.objects.get(&object)?;
                        let (from, owner) = (o.zone, o.owner);
                        let new_object = self.state.new_object_id();
                        Some(Event::ZoneChange {
                            object,
                            new_object,
                            from,
                            to: ZoneRef::of(Zone::Graveyard, owner),
                            index: None,
                        })
                    })
                    .collect();
                apply::apply_simultaneous(
                    &mut self.state,
                    Cause::StateBasedAction,
                    events,
                    &mut self.log,
                );
                self.cache.invalidate();
                Ok(())
            }

            (ChoiceKind::OrderTriggers { .. }, Answer::Order(order)) => {
                if let Some((who, batch)) = self.to_order.pop_front() {
                    let ordered: Vec<PendingTrigger> = order
                        .iter()
                        .filter_map(|i| batch.get(*i).cloned())
                        .collect();
                    // Anything the answer failed to mention keeps its fired order,
                    // so a malformed answer cannot silently drop a trigger.
                    let mut final_order = ordered;
                    for (i, t) in batch.iter().enumerate() {
                        if !order.contains(&i) {
                            final_order.push(t.clone());
                        }
                    }
                    self.place_triggers(who, &final_order);
                }
                Ok(())
            }
            _ => {
                self.pending = Some(choice);
                Err(Illegal::WrongAnswerKind)
            }
        }
    }

    // ---- settle ---------------------------------------------------------

    /// Record, once, which cards in the game would be exiled instead of going to a graveyard
    /// (see `GameState::exiled_instead_of_graveyard`). Every card is in some zone from the
    /// start, so one pass over the objects finds them.
    fn note_graveyard_redirects(&mut self, cards: &dyn PrintedCards) {
        if self.redirects_noted {
            return;
        }
        self.redirects_noted = true;
        let found: Vec<mtg_core::CardId> = self
            .state
            .objects
            .values()
            .map(|o| o.card)
            .filter(|card| {
                cards.face(*card, 0).is_some_and(|f| {
                    f.abilities.iter().any(|a| {
                        matches!(
                            &a.kind,
                            mtg_ir::AbilityKind::ReplacementEffect(mtg_ir::effect::Replacement {
                                matches: mtg_ir::EventPattern::ZoneChange {
                                    who: mtg_ir::ObjectFilter::IsSelf,
                                    from: None,
                                    to: Some(Zone::Graveyard),
                                },
                                kind: mtg_ir::effect::ReplacementKind::RedirectZoneChange {
                                    to: Zone::Exile
                                },
                            })
                        )
                    })
                })
            })
            .collect();
        self.state.exiled_instead_of_graveyard.extend(found);
    }

    /// Return every card exiled "until" a permanent that has now left the battlefield.
    fn return_linked_exiles(&mut self, cards: &dyn PrintedCards) -> bool {
        let gone: Vec<(ObjectId, ObjectId)> = self
            .state
            .linked_exile
            .iter()
            .copied()
            .filter(|(source, _)| {
                self.state
                    .objects
                    .get(source)
                    .is_none_or(|o| o.zone.zone != Zone::Battlefield)
            })
            .collect();
        if gone.is_empty() {
            return false;
        }
        self.state.linked_exile.retain(|l| !gone.contains(l));
        for (source, card) in gone {
            let Some(from) = self
                .state
                .objects
                .get(&card)
                .filter(|o| o.zone.zone == Zone::Exile)
                .map(|o| o.zone)
            else {
                continue;
            };
            let new_object = self.state.new_object_id();
            apply::apply(
                &mut self.state,
                Cause::Resolution(source),
                Event::ZoneChange {
                    object: card,
                    new_object,
                    from,
                    to: ZoneRef::shared(Zone::Battlefield),
                    index: None,
                },
                &mut self.log,
            );
            resolve::entered(
                &mut self.state,
                cards,
                &mut self.log,
                new_object,
                Cause::Resolution(source),
                None,
            );
        }
        true
    }

    fn settle(&mut self, cards: &dyn PrintedCards) -> Progress {
        self.refresh_no_life_gain(cards);
        // 0. Notice what just happened (CR 603.2). Scanning here rather than inside
        //    `apply` keeps event application free of card data, and means a replayed
        //    log detects exactly the same triggers. Because settle loops, events
        //    caused by state-based actions are scanned on the next pass round.
        self.detect_triggers(cards);

        // 0.5 Cards exiled until a permanent leaves come back once it has (CR 610.3a). Not
        //     a trigger: nothing can be done in between.
        if self.return_linked_exiles(cards) {
            self.cache.invalidate();
            return Progress::Continue;
        }

        // 1. State-based actions, to a fixpoint.
        let mut check = sba::check(&self.state, cards);
        // A Saga is not sacrificed while a chapter ability of it is still queued here to
        // go on the stack (CR 714.4) — those queues are the engine's, not the state's.
        let queued: Vec<ObjectId> = self
            .to_place
            .iter()
            .map(|(_, t)| t.source)
            .chain(
                self.to_order
                    .iter()
                    .flat_map(|(_, b)| b.iter().map(|t| t.source)),
            )
            .collect();
        check.actions.retain(|a| {
            !matches!(a, sba::Sba::PutIntoGraveyard { object, rule: "704.5s" } if queued.contains(object))
        });
        if !check.actions.is_empty() {
            self.perform_sbas(cards, check.actions);
            self.cache.invalidate();
            return Progress::Continue;
        }
        if let Some(conflict) = check.legend_conflicts.first() {
            // The legend rule needs its controller to pick a survivor. Emitted as a
            // real choice rather than resolved arbitrarily.
            let c = self.new_choice(
                conflict.controller,
                ChoiceKind::KeepOneLegend {
                    name: conflict.name.clone(),
                    candidates: conflict.candidates.clone(),
                },
                format!(
                    "the legend rule: you control more than one {}",
                    conflict.name
                )
                .into(),
                None,
            );
            return Progress::NeedsChoice(c);
        }

        // 1.5 Poll state triggers (CR 603.8). Checked at the same moment as state-based
        //     actions, because both are the engine noticing the world changed rather
        //     than reacting to something that happened.
        if self.poll_state_triggers(cards) {
            return Progress::Continue;
        }

        // 2. Put one queued trigger on the stack. One at a time, because choosing its
        //    targets can need a question and the rest of the batch waits behind it.
        if !self.to_place.is_empty() {
            if let Some(choice) = self.place_next_trigger(cards) {
                return Progress::NeedsChoice(choice);
            }
            return Progress::Continue;
        }

        // 3. With state-based actions quiet, waiting triggers go on the stack.
        if !self.state.pending_triggers.is_empty() {
            let apnap = self.state.apnap();
            let batches = self.state.pending_triggers.drain_apnap(&apnap);
            self.to_order.extend(batches);
        }

        if let Some((who, batch)) = self.to_order.front().cloned() {
            match triggers::plan_ordering(&batch) {
                // The whole point: provably order-independent, so no prompt.
                Ordering::Automatic { order, .. } => {
                    self.to_order.pop_front();
                    let ordered: Vec<PendingTrigger> = order
                        .iter()
                        .filter_map(|i| batch.get(*i).cloned())
                        .collect();
                    self.place_triggers(who, &ordered);
                    return Progress::Continue;
                }
                Ordering::MustAsk { conflicts } => {
                    let labels = batch.iter().map(|t| trigger_label(cards, t)).collect();
                    let c = self.new_choice(
                        who,
                        ChoiceKind::OrderTriggers {
                            triggers: (0..batch.len()).collect(),
                            conflicts,
                            labels,
                        },
                        "these triggers interact, so the order changes the result".into(),
                        None,
                    );
                    return Progress::NeedsChoice(c);
                }
            }
        }

        // 4. Nothing left to settle.
        self.phase = if self.state.step.grants_priority() {
            Phase::Priority
        } else {
            Phase::EndStep
        };
        Progress::Continue
    }

    /// Fire any state trigger whose condition has become true.
    ///
    /// Returns whether anything fired, so the settle loop can go round again — a state
    /// trigger's effect may itself make a state-based action apply.
    fn poll_state_triggers(&mut self, cards: &dyn PrintedCards) -> bool {
        let candidates = crate::detect::state_triggers(&self.state, cards);
        let mut fired = false;

        for t in candidates {
            // CR 603.8 — a state trigger does not trigger again while it is already
            // waiting to be put on the stack or is on the stack. Without this, a
            // condition that stays true after the ability resolves would re-fire on
            // every settle pass and the game would never reach priority.
            if self.state_trigger_is_live(t.source, t.ability) {
                continue;
            }
            self.state.pending_triggers.push(t);
            fired = true;
        }

        fired
    }

    /// Whether this state trigger has already fired and not yet finished.
    ///
    /// Derived from the three places such an ability can be sitting rather than tracked
    /// in a separate set, so it cannot fall out of sync with reality.
    fn state_trigger_is_live(&self, source: ObjectId, ability: mtg_core::AbilityId) -> bool {
        let waiting = self
            .state
            .pending_triggers
            .pending
            .iter()
            .any(|p| p.source == source && p.ability == ability);
        let queued = self
            .to_place
            .iter()
            .any(|(_, p)| p.source == source && p.ability == ability);
        let on_stack = self
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .iter()
            .any(|id| {
                self.state
                    .objects
                    .get(id)
                    .and_then(|o| o.cast_context.as_ref())
                    .is_some_and(|c| c.ability == Some(ability) && c.source == Some(source))
            });

        waiting || queued || on_stack
    }

    /// Who can't gain life right now, from "players can't gain life" statics.
    fn refresh_no_life_gain(&mut self, cards: &dyn PrintedCards) {
        let mut out = Vec::new();
        for e in crate::layers::effects(&self.state, cards) {
            if let mtg_ir::effect::Modification::Restriction(
                mtg_ir::effect::Restriction::CantGainLife(who),
            ) = &e.modification
                && let Some(controller) = crate::layers::controller(&self.state, e.source)
            {
                let chars = crate::eval::ComputedChars(cards);
                let ctx = crate::eval::Ctx {
                    state: &self.state,
                    cards,
                    chars: &chars,
                    source: e.source,
                    controller,
                    targets: &[],
                    target_legal: &[],
                    x: 0,
                    bindings: crate::empty_bindings(),
                };
                out.extend(crate::eval::players(&ctx, who).unwrap_or_default());
            }
        }
        out.sort();
        out.dedup();
        self.state.no_life_gain = out;
        let mut boost: std::collections::BTreeMap<PlayerId, (i32, u32)> = Default::default();
        for e in crate::layers::effects(&self.state, cards) {
            if let mtg_ir::effect::Modification::Restriction(
                mtg_ir::effect::Restriction::LifeGainBoost { who, plus, double },
            ) = &e.modification
                && let Some(controller) = crate::layers::controller(&self.state, e.source)
            {
                let chars = crate::eval::ComputedChars(cards);
                let ctx = crate::eval::Ctx {
                    state: &self.state,
                    cards,
                    chars: &chars,
                    source: e.source,
                    controller,
                    targets: &[],
                    target_legal: &[],
                    x: 0,
                    bindings: crate::empty_bindings(),
                };
                for p in crate::eval::players(&ctx, who).unwrap_or_default() {
                    let entry = boost.entry(p).or_default();
                    entry.0 += plus;
                    entry.1 += u32::from(*double);
                }
            }
        }
        self.state.life_gain_boost = boost;
        // "You may look at the top card of your library any time", "play with the top card
        // of your library revealed": their controllers.
        let (mut sees, mut revealed) = (Vec::new(), Vec::new());
        for e in crate::layers::effects(&self.state, cards) {
            let list = match &e.modification {
                mtg_ir::effect::Modification::Restriction(
                    mtg_ir::effect::Restriction::LookAtTopOfLibrary,
                ) => &mut sees,
                mtg_ir::effect::Modification::Restriction(
                    mtg_ir::effect::Restriction::TopOfLibraryRevealed,
                ) => &mut revealed,
                _ => continue,
            };
            if let Some(p) = crate::layers::controller(&self.state, e.source)
                && !list.contains(&p)
            {
                list.push(p);
            }
        }
        self.state.sees_top = sees;
        self.state.top_revealed = revealed;
    }

    /// Speed (CR 702.179): a player with a "start your engines!" permanent has speed 1 at
    /// least; it increases once each turn when an opponent loses life during that player's
    /// turn, to at most 4. Applied as the events are noticed.
    fn update_speed(&mut self, cards: &dyn PrintedCards) {
        let starters: Vec<PlayerId> = self
            .state
            .battlefield()
            .into_iter()
            .filter(|id| {
                crate::layers::compute(&self.state, cards, *id).is_some_and(|ch| {
                    ch.granted_keywords
                        .contains(&mtg_core::Keyword::StartYourEngines)
                        || self.state.objects.get(id).is_some_and(|o| {
                            cards.face(o.card, o.face).is_some_and(|f| {
                                f.abilities.iter().any(|a| {
                                    ch.abilities.contains(&a.id)
                                        && matches!(
                                            a.kind,
                                            mtg_ir::AbilityKind::Keyword(
                                                mtg_core::Keyword::StartYourEngines
                                            )
                                        )
                                })
                            })
                        })
                })
            })
            .filter_map(|id| crate::layers::controller(&self.state, id))
            .collect();
        for player in starters {
            if self.state.player(player).speed.is_none() {
                apply::apply(
                    &mut self.state,
                    Cause::TurnStructure,
                    Event::SpeedChanged { player, speed: 1 },
                    &mut self.log,
                );
            }
        }
        let active = self.state.active_player;
        let from = self.state.scanned_upto.min(self.log.len());
        let opponent_lost = self.log[from..].iter().any(|e| match &e.event {
            Event::LifeChanged { player, delta } => *player != active && *delta < 0,
            Event::DamageDealtToPlayer {
                player,
                amount,
                counters,
                ..
            } => *player != active && *amount > 0 && !*counters,
            _ => false,
        });
        let p = self.state.player(active);
        if opponent_lost
            && let Some(speed) = p.speed
            && speed < 4
            && p.speed_raised_turn != self.state.turn
        {
            apply::apply(
                &mut self.state,
                Cause::TurnStructure,
                Event::SpeedChanged {
                    player: active,
                    speed: speed + 1,
                },
                &mut self.log,
            );
        }
    }

    /// Scan newly logged events for triggered abilities.
    fn detect_triggers(&mut self, cards: &dyn PrintedCards) {
        self.update_speed(cards);
        if self.state.scanned_upto >= self.log.len() {
            return;
        }
        let fired = crate::detect::detect(&self.state, cards, &self.log);
        self.state.scanned_upto = self.log.len();
        for t in fired {
            // A delayed trigger that has fired is used up (CR 603.7c).
            if let Some((id, _)) = &t.delayed {
                self.state.delayed.retain(|d| d.id != *id);
            }
            self.state.pending_triggers.push(t);
        }
    }

    fn perform_sbas(&mut self, cards: &dyn PrintedCards, actions: Vec<Sba>) {
        let mut events = Vec::new();
        for a in actions {
            match a {
                Sba::PlayerLoses { player, reason } => {
                    events.push(Event::Lost { player, reason });
                }
                // CR 704.5g/h destroy, which indestructible and regeneration can stop.
                Sba::Destroy { object, .. } => {
                    for mut e in resolve::destruction(&self.state, cards, object) {
                        if let Event::ZoneChange { new_object, .. } = &mut e {
                            *new_object = self.state.new_object_id();
                        }
                        events.push(e);
                    }
                }
                Sba::PutIntoGraveyard { object, .. } => {
                    if let Some(o) = self.state.objects.get(&object) {
                        let (from, owner) = (o.zone, o.owner);
                        let new_object = self.state.new_object_id();
                        events.push(Event::ZoneChange {
                            object,
                            new_object,
                            from,
                            to: ZoneRef::of(Zone::Graveyard, owner),
                            index: None,
                        });
                    }
                }
                Sba::Unattach { object } => {
                    events.push(Event::Attached { object, to: None });
                }
                Sba::ReturnCommander { object } => {
                    if let Some(o) = self.state.objects.get(&object) {
                        let (from, owner) = (o.zone, o.owner);
                        let new_object = self.state.new_object_id();
                        events.push(Event::ZoneChange {
                            object,
                            new_object,
                            from,
                            to: ZoneRef::of(Zone::Command, owner),
                            index: None,
                        });
                    }
                }
                Sba::CeaseToExist { object } => {
                    if self.state.objects.contains_key(&object) {
                        events.push(Event::CeasedToExist { object });
                    }
                }
            }
        }
        // CR 704.3 — all of them at once, sharing one timestamp.
        apply::apply_simultaneous(
            &mut self.state,
            Cause::StateBasedAction,
            events,
            &mut self.log,
        );
    }

    /// Put an ability on the stack as an object in its own right (CR 113.7).
    fn push_ability_on_stack(
        &mut self,
        cards: &dyn PrintedCards,
        source: ObjectId,
        ability: mtg_core::AbilityId,
        controller: PlayerId,
    ) -> Option<ObjectId> {
        let granted = ability
            .granted_index()
            .and_then(|_| crate::abilities::find(&self.state, cards, source, ability))
            .map(|a| Box::new(a.into_owned()));
        let object = self.state.new_object_id();
        apply::apply(
            &mut self.state,
            Cause::PlayerAction(controller),
            Event::AbilityPutOnStack {
                object,
                source,
                ability,
                controller,
            },
            &mut self.log,
        );

        let src = self.state.objects.get(&source).cloned()?;
        let mut stack_obj = src;
        stack_obj.id = object;
        stack_obj.zone = ZoneRef::shared(Zone::Stack);
        // An ability is not a token even when its source is one.
        stack_obj.is_token = false;
        stack_obj.cast_context = Some(crate::state::CastContext {
            ability: Some(ability),
            source: Some(source),
            granted,
            ..Default::default()
        });
        self.state.objects.insert(object, stack_obj);
        self.state
            .zone_order
            .entry(ZoneRef::shared(Zone::Stack))
            .or_default()
            .insert(0, object);
        Some(object)
    }

    /// Queue an ordered batch of triggers for placement.
    ///
    /// Placement itself happens one at a time in [`Engine::place_next_trigger`], because
    /// each may need targets chosen.
    fn place_triggers(&mut self, controller: PlayerId, ordered: &[PendingTrigger]) {
        for t in ordered {
            self.to_place.push_back((controller, t.clone()));
        }
    }

    /// Put the next queued trigger on the stack, and begin its announcement if it
    /// targets.
    ///
    /// Returns a choice when targets must be chosen. `None` means the queue is empty or
    /// the trigger needed nothing.
    fn place_next_trigger(&mut self, cards: &dyn PrintedCards) -> Option<Choice> {
        while let Some((controller, t)) = self.to_place.pop_front() {
            // CR 603.4 — an "intervening if" clause is checked when the ability would
            // go on the stack, and again on resolution. Failing it here means the
            // ability simply never goes on the stack.
            if !self.intervening_if_holds(cards, &t) {
                continue;
            }

            // Count it against any once-per-turn limit only once it actually goes on
            // the stack.
            *self
                .state
                .triggered_this_turn
                .entry((t.source, t.ability))
                .or_insert(0) += 1;

            // Snapshot before the stack insert: CR 603.3d removes the ability from the
            // stack if it cannot choose legal targets, and rewinding is how that is
            // expressed here.
            let snapshot = Box::new(self.state.clone());
            let log_len = self.log.len();

            let object = self.state.new_object_id();
            apply::apply(
                &mut self.state,
                Cause::Resolution(t.source),
                Event::AbilityPutOnStack {
                    object,
                    source: t.source,
                    ability: t.ability,
                    controller,
                },
                &mut self.log,
            );

            // The ability becomes an object on the stack in its own right (CR 113.7).
            // Its source may already have left the battlefield — a dies trigger, say —
            // so last-known information stands in for the characteristics.
            // A delayed trigger's source may be a spell long gone from every zone; it
            // still needs a stack object of its own.
            let src = self
                .state
                .objects
                .get(&t.source)
                .or_else(|| self.state.last_known.get(&t.source))
                .cloned()
                .or_else(|| {
                    t.delayed.as_ref().map(|_| {
                        crate::state::GameObject::new(
                            object,
                            t.card,
                            controller,
                            ZoneRef::shared(Zone::Stack),
                        )
                    })
                });
            if let Some(src) = src {
                let mut stack_obj = src;
                stack_obj.face = t.face;
                stack_obj.id = object;
                stack_obj.zone = ZoneRef::shared(Zone::Stack);
                // An ability is not a token even when its source is one.
                stack_obj.is_token = false;
                stack_obj.controller = controller;
                stack_obj.cast_context = Some(crate::state::CastContext {
                    ability: Some(t.ability),
                    source: Some(t.source),
                    bindings: t.bindings.clone(),
                    x: crate::detect::event_amount(&t.cause.event),
                    effect: t.delayed.as_ref().map(|(_, e)| e.clone()),
                    granted: t.granted.clone(),
                    ..Default::default()
                });
                self.state.objects.insert(object, stack_obj);
                self.state
                    .zone_order
                    .entry(ZoneRef::shared(Zone::Stack))
                    .or_default()
                    .insert(0, object);
            }

            // A triggered ability pays no cost, so announcement here is targets only.
            if self.begin_announcement(cards, object, controller, false, snapshot, log_len) {
                self.phase = Phase::Announcing;
                return self.drive_announcement(cards);
            }
        }
        None
    }

    /// Evaluate a trigger's intervening-if clause, if it has one (CR 603.4).
    ///
    /// Note this is *not* a cost and not a targeting requirement: it is a plain
    /// condition, and a trigger whose condition has stopped being true is simply
    /// discarded rather than fizzling on the stack.
    fn intervening_if_holds(&self, cards: &dyn PrintedCards, t: &PendingTrigger) -> bool {
        let Some(obj) = self
            .state
            .objects
            .get(&t.source)
            .or_else(|| self.state.last_known.get(&t.source))
        else {
            return true;
        };
        let Some(face) = cards.face(obj.card, t.face) else {
            return true;
        };
        let Some(ability) = t
            .granted
            .as_deref()
            .or_else(|| face.abilities.iter().find(|a| a.id == t.ability))
        else {
            return true;
        };
        let mtg_ir::AbilityKind::Triggered { trigger, .. } = &ability.kind else {
            return true;
        };
        let Some(cond) = &trigger.intervening_if else {
            return true;
        };

        let chars = crate::eval::ComputedChars(cards);
        let ctx = crate::eval::Ctx {
            state: &self.state,
            cards,
            chars: &chars,
            source: t.source,
            controller: t.controller,
            targets: &[],
            target_legal: &[],
            x: 0,
            bindings: &t.bindings,
        };
        crate::eval::condition(&ctx, cond).unwrap_or(true)
    }

    // ---- priority ---------------------------------------    // ---- priority -------------------------------------------------------

    fn give_priority(&mut self, cards: &dyn PrintedCards) -> Progress {
        let who = self.state.priority.unwrap_or(self.state.active_player);
        let who = if self.state.player(who).has_lost {
            turn::next_player(&self.state, who)
        } else {
            who
        };

        let legal = self.legal_actions(cards, who);
        let c = self.new_choice(
            who,
            ChoiceKind::Priority { legal },
            format!("priority in {:?}", self.state.step).into(),
            Some(Answer::Pass),
        );
        Progress::NeedsChoice(c)
    }

    fn record_pass(&mut self, who: PlayerId) {
        if let Some(p) = self.state.players.get_mut(&who) {
            p.passed = true;
        }
        self.state.consecutive_passes += 1;

        let living = turn::living(&self.state).len() as u8;
        if self.state.consecutive_passes >= living {
            self.state.consecutive_passes = 0;
            // CR 117.4: everyone passed in succession. The top of the stack
            // resolves, or the step ends if the stack is empty.
            let stack_empty = self
                .state
                .objects_in(ZoneRef::shared(Zone::Stack))
                .is_empty();
            self.phase = if stack_empty {
                Phase::EndStep
            } else {
                Phase::ResolveTop
            };
        } else {
            let next = turn::next_player(&self.state, who);
            self.state.priority = Some(next);
            self.phase = Phase::Priority;
        }
    }

    fn take_action(&mut self, cards: &dyn PrintedCards, who: PlayerId, action: Action) {
        match action {
            Action::Pass => self.record_pass(who),
            Action::Concede => {
                apply::apply(
                    &mut self.state,
                    Cause::PlayerAction(who),
                    Event::Lost {
                        player: who,
                        reason: LossReason::Concede,
                    },
                    &mut self.log,
                );
                self.phase = Phase::Settle;
            }
            Action::PlayLand { object } | Action::PlayLandFace { object, .. } => {
                let face = action.play().map_or(0, |(_, face, _)| face);
                // CR 305.1 — a special action. It uses no stack and cannot be
                // responded to, so the land simply arrives.
                if let Some(o) = self.state.objects.get(&object) {
                    let from = o.zone;
                    let new_object = self.state.new_object_id();
                    apply::apply(
                        &mut self.state,
                        Cause::PlayerAction(who),
                        Event::ZoneChange {
                            object,
                            new_object,
                            from,
                            to: ZoneRef::shared(Zone::Battlefield),
                            index: None,
                        },
                        &mut self.log,
                    );
                    apply::apply(
                        &mut self.state,
                        Cause::PlayerAction(who),
                        Event::FaceSelected {
                            object: new_object,
                            face,
                        },
                        &mut self.log,
                    );
                    // "As this land enters, you may pay 2 life": asked before anything
                    // else can happen, then it finishes entering.
                    if let Some((kind, question, default)) =
                        resolve::enter_question(&self.state, cards, new_object)
                    {
                        let c = self.new_choice(who, kind, question.into(), Some(default));
                        self.entering_choice = Some((c.id, new_object));
                    } else {
                        resolve::entered(
                            &mut self.state,
                            cards,
                            &mut self.log,
                            new_object,
                            Cause::PlayerAction(who),
                            None,
                        );
                    }
                }
                if let Some(p) = self.state.players.get_mut(&who) {
                    p.lands_played += 1;
                }
                // Playing a land does not pass priority.
                self.state.consecutive_passes = 0;
                self.phase = Phase::Settle;
            }
            // CR 702.37e — turning a face-down permanent face up is a special action: pay
            // the morph cost, no stack, no priority passed.
            Action::SpecialAction { source, ability }
                if morph_of(&self.state, cards, source, ability).is_some() =>
            {
                if let Some((cost, megamorph)) = morph_of(&self.state, cards, source, ability)
                    && let Some(plan) = crate::mana::plan(&self.state, cards, who, &cost.mana, 0)
                {
                    self.pay(cards, who, &plan);
                    apply::apply(
                        &mut self.state,
                        Cause::PlayerAction(who),
                        Event::FaceDownChanged {
                            object: source,
                            face_down: false,
                        },
                        &mut self.log,
                    );
                    if megamorph {
                        apply::apply(
                            &mut self.state,
                            Cause::PlayerAction(who),
                            Event::CountersChanged {
                                object: source,
                                kind: mtg_core::CounterKind::PlusOnePlusOne,
                                delta: 1,
                            },
                            &mut self.log,
                        );
                    }
                }
                self.state.consecutive_passes = 0;
                self.cache.invalidate();
                self.phase = Phase::Settle;
            }
            Action::SpecialAction { source, ability }
                if exile_later_of(&self.state, cards, source, ability).is_some() =>
            {
                if let Some((cost, face_down, later, sorcery)) =
                    exile_later_of(&self.state, cards, source, ability)
                    && let Some(plan) = crate::mana::plan(&self.state, cards, who, &cost.mana, 0)
                    && let Some(from) = self.state.objects.get(&source).map(|o| o.zone)
                {
                    self.pay(cards, who, &plan);
                    let new_object = self.state.new_object_id();
                    apply::apply(
                        &mut self.state,
                        Cause::PlayerAction(who),
                        Event::ZoneChange {
                            object: source,
                            new_object,
                            from,
                            to: ZoneRef::shared(Zone::Exile),
                            index: None,
                        },
                        &mut self.log,
                    );
                    if face_down {
                        apply::apply(
                            &mut self.state,
                            Cause::PlayerAction(who),
                            Event::FaceDownChanged {
                                object: new_object,
                                face_down: true,
                            },
                            &mut self.log,
                        );
                    }
                    let after_turn = self.state.turn;
                    apply::apply(
                        &mut self.state,
                        Cause::PlayerAction(who),
                        Event::CastLater {
                            object: new_object,
                            player: who,
                            after_turn,
                            cost: later,
                            sorcery,
                        },
                        &mut self.log,
                    );
                }
                self.state.consecutive_passes = 0;
                self.cache.invalidate();
                self.phase = Phase::Settle;
            }
            Action::SpecialAction { source, ability }
                if suspend_of(&self.state, cards, source, ability).is_some() =>
            {
                if let Some((time, cost, kind)) = suspend_of(&self.state, cards, source, ability)
                    && let Some(plan) = crate::mana::plan(&self.state, cards, who, &cost.mana, 0)
                    && let Some(from) = self.state.objects.get(&source).map(|o| o.zone)
                {
                    self.pay(cards, who, &plan);
                    let new_object = self.state.new_object_id();
                    apply::apply(
                        &mut self.state,
                        Cause::PlayerAction(who),
                        Event::ZoneChange {
                            object: source,
                            new_object,
                            from,
                            to: ZoneRef::shared(Zone::Exile),
                            index: None,
                        },
                        &mut self.log,
                    );
                    apply::apply(
                        &mut self.state,
                        Cause::PlayerAction(who),
                        Event::CountersChanged {
                            object: new_object,
                            kind,
                            delta: time as i32,
                        },
                        &mut self.log,
                    );
                }
                self.state.consecutive_passes = 0;
                self.cache.invalidate();
                self.phase = Phase::Settle;
            }
            Action::CastAlternative { object, ability } => {
                let snapshot = Box::new(self.state.clone());
                let log_len = self.log.len();
                let alt = self
                    .state
                    .objects
                    .get(&object)
                    .and_then(|o| cards.face(o.card, o.face))
                    .and_then(|f| f.abilities.iter().find(|a| a.id == ability))
                    .and_then(|a| match &a.kind {
                        mtg_ir::AbilityKind::AlternativeCost {
                            cost,
                            kind,
                            instead,
                        } => Some((cost.mana.clone(), *kind, instead.clone())),
                        _ => None,
                    });
                let new_object = self.state.new_object_id();
                if let Some((mana, kind, instead)) = alt
                    && let Some(from) = self.state.objects.get(&object).map(|o| o.zone)
                {
                    apply::apply(
                        &mut self.state,
                        Cause::PlayerAction(who),
                        Event::ZoneChange {
                            object,
                            new_object,
                            from,
                            to: ZoneRef::shared(Zone::Stack),
                            index: Some(0),
                        },
                        &mut self.log,
                    );
                    if let Some(o) = self.state.objects.get_mut(&new_object) {
                        let cc = o.cast_context.get_or_insert_with(Default::default);
                        cc.cost_override = Some(mana);
                        cc.alt_cost = Some(kind);
                        // Overload: the spell does its "each" version.
                        if let Some(instead) = instead {
                            cc.effect = Some(*instead);
                        }
                        cc.cast_from = Some(from.zone);
                    }
                }
                if self.begin_announcement(cards, new_object, who, true, snapshot, log_len) {
                    self.phase = Phase::Announcing;
                } else {
                    self.phase = Phase::Settle;
                }
            }
            Action::CastFaceDown { object } => {
                let snapshot = Box::new(self.state.clone());
                let log_len = self.log.len();
                let new_object = self.state.new_object_id();
                if let Some(from) = self.state.objects.get(&object).map(|o| o.zone) {
                    apply::apply(
                        &mut self.state,
                        Cause::PlayerAction(who),
                        Event::ZoneChange {
                            object,
                            new_object,
                            from,
                            to: ZoneRef::shared(Zone::Stack),
                            index: Some(0),
                        },
                        &mut self.log,
                    );
                    apply::apply(
                        &mut self.state,
                        Cause::PlayerAction(who),
                        Event::FaceDownChanged {
                            object: new_object,
                            face_down: true,
                        },
                        &mut self.log,
                    );
                    if let Some(o) = self.state.objects.get_mut(&new_object) {
                        let cc = o.cast_context.get_or_insert_with(Default::default);
                        cc.cost_override = Some(face_down_cost());
                        cc.cast_from = Some(from.zone);
                    }
                }
                if self.begin_announcement(cards, new_object, who, true, snapshot, log_len) {
                    self.phase = Phase::Announcing;
                } else {
                    self.phase = Phase::Settle;
                }
            }
            Action::ActivateAbility { source, ability }
            | Action::SpecialAction { source, ability } => {
                let snapshot = Box::new(self.state.clone());
                let log_len = self.log.len();

                // The ability becomes an object on the stack in its own right
                // (CR 113.7). This previously emitted the event without creating the
                // object, so activated abilities never actually reached the stack.
                let object = self.push_ability_on_stack(cards, source, ability, who);

                // A mana ability uses no stack at all (CR 605.3) and arrives as
                // `ActivateManaAbility` or through payment, so anything reaching here is a
                // normal activated ability.
                if let Some(object) = object
                    && self.begin_announcement(cards, object, who, false, snapshot, log_len)
                {
                    self.phase = Phase::Announcing;
                    return;
                }
                self.state.consecutive_passes = 0;
                self.phase = Phase::Settle;
            }
            Action::ActivateManaAbility {
                source,
                ability,
                color,
            } => {
                // CR 605.3b: no stack, resolves immediately, and the player keeps
                // priority. Settling afterwards lets anything that triggered on the tap
                // go on the stack before priority is given back.
                self.activate_mana_ability(cards, who, source, ability, color);
                self.phase = Phase::Settle;
            }
            Action::Cast { object } | Action::CastFace { object, .. } => {
                let face = action.play().map_or(0, |(_, face, _)| face);
                // CR 601.2 in order: the spell goes on the stack (601.2a), targets are
                // chosen (601.2c), the cost is determined while it is there (601.2f),
                // and only then is it paid (601.2h). The snapshot is taken first so a
                // legally-incompletable announcement rewinds entirely.
                let snapshot = Box::new(self.state.clone());
                let log_len = self.log.len();

                let new_object = self.state.new_object_id();
                let cast_from = self.state.objects.get(&object).map(|o| o.zone.zone);
                // Foretold or plotted: cast for its later cost (or for free).
                let later = self
                    .state
                    .objects
                    .get(&object)
                    .filter(|o| o.zone.zone == Zone::Exile)
                    .and_then(|o| o.cast_later.as_ref())
                    .map(|(_, _, cost, _)| cost.clone().unwrap_or_default());
                // Cast from the graveyard for its alternative cost (flashback).
                let alternative = self
                    .state
                    .objects
                    .get(&object)
                    .filter(|o| o.zone.zone == Zone::Graveyard)
                    .and_then(|o| crate::cost::graveyard_cast(cards, o.card, face));
                if let Some(o) = self.state.objects.get(&object) {
                    let from = o.zone;
                    // CR 903.8: the tax is fixed as the commander leaves the command zone,
                    // and the next cast from there costs {2} more.
                    if from.zone == Zone::Command {
                        let tax = self.state.commander.tax(o.owner);
                        self.state.commander.tax_due.insert(new_object, tax);
                        *self.state.commander.casts.entry(o.owner).or_insert(0) += 1;
                    }
                    apply::apply(
                        &mut self.state,
                        Cause::PlayerAction(who),
                        Event::ZoneChange {
                            object,
                            new_object,
                            from,
                            to: ZoneRef::shared(Zone::Stack),
                            index: Some(0),
                        },
                        &mut self.log,
                    );
                }
                apply::apply(
                    &mut self.state,
                    Cause::PlayerAction(who),
                    Event::FaceSelected {
                        object: new_object,
                        face,
                    },
                    &mut self.log,
                );
                if let Some(o) = self.state.objects.get_mut(&new_object) {
                    o.cast_context
                        .get_or_insert_with(Default::default)
                        .cast_from = cast_from;
                }
                if let Some((cost, exile)) = alternative
                    && let Some(o) = self.state.objects.get_mut(&new_object)
                {
                    let cc = o.cast_context.get_or_insert_with(Default::default);
                    cc.cost_override = Some(cost);
                    cc.exile_on_leave = exile;
                }
                if let Some(cost) = later
                    && let Some(o) = self.state.objects.get_mut(&new_object)
                {
                    o.cast_context
                        .get_or_insert_with(Default::default)
                        .cost_override = Some(cost);
                }

                if self.begin_announcement(cards, new_object, who, true, snapshot, log_len) {
                    self.phase = Phase::Announcing;
                } else {
                    self.phase = Phase::Settle;
                }
            }
        }
    }

    /// What a player may legally do right now.
    ///
    /// A spell appears here only if its cost — after modifiers — can actually be
    /// paid, worked out by the same planner that will pay it
    /// ([`crate::mana::plan`]). Using one algorithm for both means the list can
    /// never offer a spell that then turns out to be unpayable.
    fn legal_actions(&self, cards: &dyn PrintedCards, who: PlayerId) -> LegalActions {
        let mut actions = vec![Action::Pass, Action::Concede];
        let mut mana_abilities = Vec::new();

        let stack_empty = self
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty();
        let sorcery_time =
            self.state.active_player == who && self.state.step.is_main_phase() && stack_empty;
        // One land a turn (CR 305.2), and one more for each "you may play an additional
        // land" this player controls.
        let land_plays = 1 + crate::layers::effects(&self.state, cards)
            .iter()
            .filter(|e| {
                matches!(
                    e.modification,
                    mtg_ir::effect::Modification::Restriction(
                        mtg_ir::effect::Restriction::AdditionalLandPlay
                    )
                ) && e
                    .controller
                    .or_else(|| crate::layers::controller(&self.state, e.source))
                    == Some(who)
            })
            .count() as u32;

        // CR 903.8: a commander may be cast from the command zone, as if from hand.
        let castable_from = self
            .state
            .objects_in(ZoneRef::of(Zone::Hand, who))
            .into_iter()
            .chain(
                self.state
                    .objects_in(ZoneRef::of(Zone::Command, who))
                    .into_iter()
                    .filter(|id| {
                        self.state
                            .objects
                            .get(id)
                            .is_some_and(|o| self.state.commander.is_commander(o.owner, o.card))
                    }),
            )
            .collect::<Vec<_>>();
        // Cards castable from the graveyard (flashback).
        let castable_from: Vec<ObjectId> = castable_from
            .into_iter()
            .chain(
                self.state
                    .objects_in(ZoneRef::of(Zone::Graveyard, who))
                    .into_iter()
                    .filter(|id| {
                        self.state.objects.get(id).is_some_and(|o| {
                            (0..2).any(|face| {
                                crate::cost::graveyard_cast(cards, o.card, face).is_some()
                            })
                        })
                    }),
            )
            .collect();
        // "You may cast creature spells from the top of your library": the top card, when
        // some such effect of this player's allows it.
        let from_top: Vec<(mtg_ir::ObjectFilter, bool)> =
            crate::layers::effects(&self.state, cards)
                .iter()
                .filter(|e| crate::layers::controller(&self.state, e.source) == Some(who))
                .filter_map(|e| match &e.modification {
                    mtg_ir::effect::Modification::Restriction(
                        mtg_ir::effect::Restriction::PlayFromTopOfLibrary { spells, lands },
                    ) => Some((spells.clone(), *lands)),
                    _ => None,
                })
                .collect();
        let top = self
            .state
            .objects_in(ZoneRef::of(Zone::Library, who))
            .first()
            .copied()
            .filter(|_| !from_top.is_empty());
        let castable_from: Vec<ObjectId> = castable_from.into_iter().chain(top).collect();
        let turn = self.state.turn;
        let may_play = |o: &crate::state::GameObject| {
            o.may_play
                .is_some_and(|(p, until, _)| p == who && turn <= until)
        };
        let castable_from = castable_from.into_iter().chain(
            self.state
                .objects_in(ZoneRef::shared(Zone::Exile))
                .into_iter()
                .filter(|id| {
                    let o = &self.state.objects[id];
                    o.adventure_player == Some(who)
                        || may_play(o)
                        || o.cast_later.as_ref().is_some_and(|(p, after, _, sorcery)| {
                            *p == who && turn > *after && (!*sorcery || sorcery_time)
                        })
                }),
        );
        for id in castable_from {
            let obj = &self.state.objects[&id];
            // A transforming card's back face is cast only with disturb, from a graveyard.
            let count = if matches!(
                cards.layout(obj.card),
                mtg_ir::Layout::ModalDfc | mtg_ir::Layout::Adventure | mtg_ir::Layout::Split
            ) || (obj.zone.zone == Zone::Graveyard
                && crate::cost::graveyard_cast(cards, obj.card, 1).is_some())
            {
                2
            } else {
                1
            };
            for face in 0..count {
                if obj.zone.zone != Zone::Graveyard
                    && cards.face(obj.card, face).is_some_and(|f| {
                        f.abilities
                            .iter()
                            .any(|a| matches!(a.kind, mtg_ir::AbilityKind::Aftermath))
                    })
                {
                    continue;
                }
                // An Adventurer card in exile is cast as its creature (CR 715.3d).
                if obj.zone.zone == Zone::Exile && face != 0 && obj.adventure_player == Some(who) {
                    continue;
                }
                // Evaluate only the chosen face (CR 712.11c), without mutating
                // the live card's front-face characteristics in its current zone.
                let mut selected;
                // A foretold card is face down in exile, but is cast as itself.
                let state = if face == 0
                    && cards.layout(obj.card) != mtg_ir::Layout::Split
                    && !obj.face_down
                {
                    &self.state
                } else {
                    selected = self.state.clone();
                    let object = selected.objects.get_mut(&id).unwrap();
                    object.face = face;
                    object.face_down = false;
                    // A split card has combined characteristics in its old zone, but
                    // casting evaluates only the chosen half (CR 709.3a).
                    object.cast_context.get_or_insert_with(Default::default);
                    &selected
                };
                if obj.zone.zone == Zone::Graveyard
                    && cards.face(obj.card, face).is_none_or(|f| {
                        crate::cost::graveyard_cast(cards, obj.card, face).is_none()
                            || !crate::cost::additional_payable(
                                state,
                                cards,
                                id,
                                who,
                                &crate::cost::cast_from_additional(f, Zone::Graveyard),
                            )
                    })
                {
                    continue;
                }

                let Some(ch) = crate::layers::compute(state, cards, id) else {
                    continue;
                };

                // A land is played, not cast, and only one per turn (CR 305.2).
                if ch.has_type(mtg_core::CardType::Land) {
                    if (obj.zone.zone == Zone::Hand
                        || (obj.zone.zone == Zone::Library && from_top.iter().any(|(_, l)| *l))
                        || (obj.zone.zone == Zone::Exile
                            && (obj.adventure_player == Some(who)
                                || (may_play(obj) && obj.may_play.is_some_and(|m| !m.2)))))
                        && sorcery_time
                        && state.player(who).lands_played < land_plays
                    {
                        actions.push(if face == 0 {
                            Action::PlayLand { object: id }
                        } else {
                            Action::PlayLandFace { object: id, face }
                        });
                    }
                    continue;
                }

                // From the top of the library: only the spells its permission names.
                if obj.zone.zone == Zone::Library {
                    let chars = crate::eval::ComputedChars(cards);
                    let ctx = crate::eval::Ctx {
                        state,
                        cards,
                        chars: &chars,
                        source: id,
                        controller: who,
                        targets: &[],
                        target_legal: &[],
                        x: 0,
                        bindings: crate::empty_bindings(),
                    };
                    if !from_top
                        .iter()
                        .any(|(f, _)| crate::eval::matches(&ctx, f, id).unwrap_or(false))
                    {
                        continue;
                    }
                }
                if !crate::cost::timing_allows(state, cards, id, who) {
                    continue;
                }
                // `{X}` is announced as 0 here. Offering the spell with X = 0 is correct;
                // choosing a larger X is a choice made during announcement, and that
                // flow is not wired yet.
                let Some(total) = crate::cost::total_cost(state, cards, id, who) else {
                    continue;
                };
                if crate::mana::plan_spell(state, cards, who, &total, 0, id).is_none() {
                    continue;
                }
                if let Some(extra) = cards
                    .face(obj_card(state, id), face)
                    .and_then(crate::cost::additional_cast_cost)
                    && !crate::cost::additional_payable(state, cards, id, who, &extra)
                {
                    continue;
                }
                // CR 601.2c — a spell whose required target has no legal choice cannot be
                // cast at all. Checked here so a player is never offered a spell they
                // cannot legally announce, and so the announcement never has to rewind.
                let specs = cards
                    .face(obj_card(state, id), face)
                    .map(|f| {
                        f.abilities
                            .iter()
                            .find(|a| {
                                matches!(
                                    a.kind,
                                    mtg_ir::AbilityKind::SpellEffect(_)
                                        | mtg_ir::AbilityKind::Enchant
                                )
                            })
                            .map(|a| a.targets.clone())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                if !specs.is_empty()
                    && !crate::targeting::can_be_announced(state, cards, &specs, id, who)
                {
                    continue;
                }
                actions.push(if face == 0 {
                    Action::Cast { object: id }
                } else {
                    Action::CastFace { object: id, face }
                });
            }
        }

        // Activated abilities that work from the hand (cycling) or the graveyard (unearth),
        // of cards in this player's hand and graveyard.
        let hand_and_graveyard = [Zone::Hand, Zone::Graveyard]
            .into_iter()
            .flat_map(|z| self.state.objects_in(ZoneRef::of(z, who)));
        for id in hand_and_graveyard.collect::<Vec<_>>() {
            let zone = self.state.objects[&id].zone.zone;
            let Some(face) = self
                .state
                .objects
                .get(&id)
                .and_then(|o| cards.face(o.card, o.face))
            else {
                continue;
            };
            for ability in &face.abilities {
                let mtg_ir::AbilityKind::Activated {
                    cost,
                    functions_from,
                    is_mana_ability: false,
                    is_loyalty_ability: false,
                    timing,
                    ..
                } = &ability.kind
                else {
                    continue;
                };
                if *functions_from != zone {
                    continue;
                }
                if !crate::cost::additional_payable(&self.state, cards, id, who, cost)
                    || !crate::mana::can_pay(&self.state, cards, who, &cost.mana, 0)
                    || (timing.sorcery_only() && !sorcery_time)
                    || !timing.allows(self.state.active_player == who, self.state.step)
                    || (*timing == mtg_ir::ability::ActivationTiming::MaxSpeed
                        && self.state.player(who).speed != Some(4))
                    || timing.per_turn().is_some_and(|n| {
                        self.state
                            .activated_this_turn
                            .get(&(id, ability.id))
                            .is_some_and(|done| *done >= n)
                    })
                    || (!ability.targets.is_empty()
                        && !crate::targeting::can_be_announced(
                            &self.state,
                            cards,
                            &ability.targets,
                            id,
                            who,
                        ))
                {
                    continue;
                }
                actions.push(Action::ActivateAbility {
                    source: id,
                    ability: ability.id,
                });
            }
        }

        // Alternative costs from the hand (dash, evoke), whenever the card could be cast.
        for id in self.state.objects_in(ZoneRef::of(Zone::Hand, who)) {
            let Some(face) = self
                .state
                .objects
                .get(&id)
                .and_then(|o| cards.face(o.card, o.face))
            else {
                continue;
            };
            let instant_speed = face.card_types.contains(&mtg_core::CardType::Instant)
                || face.abilities.iter().any(|a| {
                    matches!(
                        a.kind,
                        mtg_ir::AbilityKind::Keyword(mtg_core::Keyword::Flash)
                    )
                })
                || crate::cost::flash_permitted(&self.state, cards, id, who);
            if !(instant_speed || sorcery_time)
                || !crate::cost::cast_conditions_met(&self.state, cards, id, who)
            {
                continue;
            }
            for a in &face.abilities {
                if let mtg_ir::AbilityKind::AlternativeCost { cost, .. } = &a.kind
                    && cost.additional.is_empty()
                    && crate::mana::plan_spell(&self.state, cards, who, &cost.mana, 0, id).is_some()
                    && (a.targets.is_empty()
                        || crate::targeting::can_be_announced(
                            &self.state,
                            cards,
                            &a.targets,
                            id,
                            who,
                        ))
                {
                    actions.push(Action::CastAlternative {
                        object: id,
                        ability: a.id,
                    });
                }
            }
        }

        // Foretell (any time you have priority during your turn) and plot (as a sorcery):
        // exile from the hand to cast on a later turn.
        for id in self.state.objects_in(ZoneRef::of(Zone::Hand, who)) {
            let Some(face) = self
                .state
                .objects
                .get(&id)
                .and_then(|o| cards.face(o.card, o.face))
            else {
                continue;
            };
            for a in &face.abilities {
                if let mtg_ir::AbilityKind::ExileToCastLater { cost, sorcery, .. } = &a.kind
                    && (if *sorcery {
                        sorcery_time
                    } else {
                        self.state.active_player == who
                    })
                    && cost.additional.is_empty()
                    && crate::mana::can_pay(&self.state, cards, who, &cost.mana, 0)
                {
                    actions.push(Action::SpecialAction {
                        source: id,
                        ability: a.id,
                    });
                }
            }
        }

        // Suspend (CR 702.62a): from the hand, whenever the card could be cast.
        for id in self.state.objects_in(ZoneRef::of(Zone::Hand, who)) {
            let Some(face) = self
                .state
                .objects
                .get(&id)
                .and_then(|o| cards.face(o.card, o.face))
            else {
                continue;
            };
            let Some((ability, cost)) = face.abilities.iter().find_map(|a| match &a.kind {
                mtg_ir::AbilityKind::Suspend { cost, .. } => Some((a.id, cost)),
                _ => None,
            }) else {
                continue;
            };
            let instant_speed = face.card_types.contains(&mtg_core::CardType::Instant)
                || face.abilities.iter().any(|a| {
                    matches!(
                        a.kind,
                        mtg_ir::AbilityKind::Keyword(mtg_core::Keyword::Flash)
                    )
                })
                || crate::cost::flash_permitted(&self.state, cards, id, who);
            if (instant_speed || sorcery_time)
                && cost.additional.is_empty()
                && crate::mana::can_pay(&self.state, cards, who, &cost.mana, 0)
            {
                actions.push(Action::SpecialAction {
                    source: id,
                    ability,
                });
            }
        }

        // Morph (CR 702.37): cast face down for {3} at sorcery speed, and turn a face-down
        // permanent face up for its morph cost whenever its controller has priority.
        if sorcery_time && crate::mana::can_pay(&self.state, cards, who, &face_down_cost(), 0) {
            for id in self.state.objects_in(ZoneRef::of(Zone::Hand, who)) {
                if self
                    .state
                    .objects
                    .get(&id)
                    .and_then(|o| cards.face(o.card, o.face))
                    .and_then(morph_ability)
                    .is_some()
                {
                    actions.push(Action::CastFaceDown { object: id });
                }
            }
        }
        for id in self.state.battlefield() {
            let Some(obj) = self.state.objects.get(&id).filter(|o| o.face_down) else {
                continue;
            };
            if crate::layers::controller(&self.state, id) != Some(who) {
                continue;
            }
            if let Some(a) = cards.face(obj.card, obj.face).and_then(morph_ability)
                && let mtg_ir::AbilityKind::Morph { cost, .. } = &a.kind
                && cost.additional.is_empty()
                && crate::mana::can_pay(&self.state, cards, who, &cost.mana, 0)
            {
                actions.push(Action::SpecialAction {
                    source: id,
                    ability: a.id,
                });
            }
        }

        // Activated abilities of permanents this player controls.
        for id in self.state.battlefield() {
            if crate::layers::controller(&self.state, id) != Some(who)
                || crate::combat::abilities_blocked(&self.state, cards, id)
            {
                continue;
            }
            // Printed and granted alike (see `crate::abilities`).
            for ability in &crate::abilities::current(&self.state, cards, id) {
                let mtg_ir::AbilityKind::Activated {
                    cost,
                    is_mana_ability,
                    is_loyalty_ability,
                    timing,
                    functions_from: Zone::Battlefield,
                    ..
                } = &ability.kind
                else {
                    continue;
                };

                if *is_loyalty_ability
                    && (!sorcery_time || self.state.loyalty_activated_this_turn.contains(&id))
                {
                    continue;
                }
                // A Class gains only its next level (CR 716.2a).
                if let mtg_ir::ability::ActivationTiming::ClassLevel(n) = timing
                    && self
                        .state
                        .objects
                        .get(&id)
                        .is_none_or(|o| o.class_level.max(1) + 1 != *n)
                {
                    continue;
                }

                // Every part of the cost must be payable: a tap cost by something
                // untapped and not summoning sick (CR 302.6), and nothing the engine
                // cannot pay at all.
                if !crate::cost::additional_payable(&self.state, cards, id, who, cost) {
                    continue;
                }
                if !crate::mana::can_pay(&self.state, cards, who, &cost.mana, 0) {
                    continue;
                }
                if timing.sorcery_only() && !sorcery_time {
                    continue;
                }
                if !timing.allows(self.state.active_player == who, self.state.step)
                    || (*timing == mtg_ir::ability::ActivationTiming::MaxSpeed
                        && self.state.player(who).speed != Some(4))
                {
                    continue;
                }
                if timing.per_turn().is_some_and(|n| {
                    self.state
                        .activated_this_turn
                        .get(&(id, ability.id))
                        .is_some_and(|done| *done >= n)
                }) {
                    continue;
                }

                if !ability.targets.is_empty()
                    && !crate::targeting::can_be_announced(
                        &self.state,
                        cards,
                        &ability.targets,
                        id,
                        who,
                    )
                {
                    continue;
                }

                if *is_mana_ability {
                    // One action per colour it could make, so the choice is made by
                    // picking one. Paid abilities are available manually; automatic
                    // payment uses only sources with no input mana or life cost.
                    let Some(source) =
                        crate::mana::manual_source(&self.state, cards, who, id, ability.id)
                    else {
                        continue;
                    };
                    for color in crate::mana::color_options(&source) {
                        mana_abilities.push(Action::ActivateManaAbility {
                            source: id,
                            ability: ability.id,
                            color,
                        });
                    }
                } else {
                    actions.push(Action::ActivateAbility {
                        source: id,
                        ability: ability.id,
                    });
                }
            }
        }

        // Split second (CR 702.61b): with such a spell on the stack, only mana abilities
        // and special actions remain.
        let split_second =
            self.state
                .objects_in(ZoneRef::shared(Zone::Stack))
                .into_iter()
                .any(|id| {
                    let ctx = crate::eval::Ctx {
                        state: &self.state,
                        cards,
                        chars: &crate::eval::ComputedChars(cards),
                        source: id,
                        controller: who,
                        targets: &[],
                        target_legal: &[],
                        x: 0,
                        bindings: crate::empty_bindings(),
                    };
                    self.state.objects.get(&id).is_some_and(|o| {
                        o.cast_context.as_ref().is_none_or(|c| c.ability.is_none())
                    }) && ctx
                        .has_keyword(id, mtg_core::Keyword::SplitSecond)
                        .unwrap_or(false)
                });
        if split_second {
            actions.retain(|a| {
                matches!(
                    a,
                    Action::Pass | Action::Concede | Action::SpecialAction { .. }
                )
            });
        }

        LegalActions {
            who: Some(who),
            actions,
            mana_abilities,
            targets: Vec::new(),
        }
    }

    /// Pay a planned cost (CR 601.2g-h).
    ///
    /// Mana abilities are activated first and **do not use the stack** (CR 605.3):
    /// they resolve immediately, here, and cannot be responded to. Their mana lands
    /// in the pool, and the cost is then taken out of the pool — there is no path by
    /// which a source pays a cost directly, which keeps a single place where mana is
    /// spent.
    fn pay(&mut self, cards: &dyn PrintedCards, who: PlayerId, plan: &crate::mana::Payment) {
        for (object, ability, color) in &plan.activate {
            if *ability == crate::mana::DELVE {
                // Delve: exiling the card is the payment, as one generic mana.
                let new_object = self.state.new_object_id();
                let owner = self.state.objects.get(object).map(|o| o.owner);
                if let Some(owner) = owner {
                    apply::apply(
                        &mut self.state,
                        Cause::CostPayment(*object),
                        Event::ZoneChange {
                            object: *object,
                            new_object,
                            from: ZoneRef::of(Zone::Graveyard, owner),
                            to: ZoneRef::shared(Zone::Exile),
                            index: None,
                        },
                        &mut self.log,
                    );
                    apply::apply(
                        &mut self.state,
                        Cause::CostPayment(*object),
                        Event::ManaAdded {
                            player: who,
                            color: None,
                            amount: 1,
                        },
                        &mut self.log,
                    );
                }
                continue;
            }
            if *ability == crate::mana::HELPER {
                // Convoke/improvise: tapping the permanent is the payment. It is modelled
                // as one mana of the colour it pays for, spent with the rest.
                apply::apply(
                    &mut self.state,
                    Cause::CostPayment(*object),
                    Event::TapChanged {
                        object: *object,
                        tapped: true,
                    },
                    &mut self.log,
                );
                apply::apply(
                    &mut self.state,
                    Cause::CostPayment(*object),
                    Event::ManaAdded {
                        player: who,
                        color: *color,
                        amount: 1,
                    },
                    &mut self.log,
                );
                continue;
            }
            self.activate_mana_ability(cards, who, *object, *ability, *color);
        }
        self.spend(who, plan);
    }

    /// Activate one mana ability: pay its tap cost, then resolve it on the spot with the
    /// chosen colour. The one path for both auto-tapping during payment and tapping by
    /// hand, so the two cannot produce different mana.
    fn activate_mana_ability(
        &mut self,
        cards: &dyn PrintedCards,
        who: PlayerId,
        object: ObjectId,
        ability: mtg_core::AbilityId,
        color: Option<mtg_core::Color>,
    ) {
        let Some((cost, effect)) = crate::abilities::find(&self.state, cards, object, ability)
            .and_then(|a| match &a.kind {
                mtg_ir::AbilityKind::Activated { cost, effect, .. } => {
                    Some((cost.clone(), effect.clone()))
                }
                _ => None,
            })
        else {
            return;
        };

        if !cost.mana.symbols.is_empty() {
            let mut payment_state = self.state.clone();
            payment_state.objects.remove(&object);
            for part in &cost.additional {
                if let mtg_ir::AdditionalCost::PayLife {
                    amount: mtg_ir::Value::Fixed(n),
                } = part
                {
                    payment_state
                        .players
                        .get_mut(&who)
                        .expect("mana controller")
                        .life -= n;
                }
            }
            let Some(plan) = crate::mana::plan(&payment_state, cards, who, &cost.mana, 0) else {
                return;
            };
            self.pay(cards, who, &plan);
        }
        for part in &cost.additional {
            if let mtg_ir::AdditionalCost::PayLife {
                amount: mtg_ir::Value::Fixed(n),
            } = part
            {
                apply::apply(
                    &mut self.state,
                    Cause::CostPayment(object),
                    Event::LifeChanged {
                        player: who,
                        delta: -*n,
                    },
                    &mut self.log,
                );
            }
        }
        apply::apply(
            &mut self.state,
            Cause::CostPayment(object),
            Event::AbilityActivated {
                source: object,
                ability,
                loyalty: false,
            },
            &mut self.log,
        );

        // The ability's own cost: tapping the permanent.
        if cost
            .additional
            .iter()
            .any(|c| matches!(c, mtg_ir::AdditionalCost::Tap { .. }))
        {
            apply::apply(
                &mut self.state,
                Cause::CostPayment(object),
                Event::TapChanged {
                    object,
                    tapped: true,
                },
                &mut self.log,
            );
        }

        resolve::sacrifice_for_mana(&mut self.state, cards, &mut self.log, object, ability);

        // Resolve immediately, making the chosen colour.
        let mut rc = ResolveCtx::new(object, who);
        rc.mana_choice = color;
        let _ = resolve::resolve(&mut self.state, cards, &mut self.log, &effect, &mut rc);
    }

    /// Take a planned payment out of the pool, and any life it calls for.
    fn spend(&mut self, who: PlayerId, plan: &crate::mana::Payment) {
        // Spend the mana.
        for (slot, amount) in plan.spend.iter().enumerate() {
            if *amount == 0 {
                continue;
            }
            let color = match slot {
                0 => Some(mtg_core::Color::White),
                1 => Some(mtg_core::Color::Blue),
                2 => Some(mtg_core::Color::Black),
                3 => Some(mtg_core::Color::Red),
                4 => Some(mtg_core::Color::Green),
                _ => None,
            };
            apply::apply(
                &mut self.state,
                Cause::CostPayment(ObjectId(0)),
                Event::ManaSpent {
                    player: who,
                    color,
                    amount: *amount,
                },
                &mut self.log,
            );
        }

        // Life for phyrexian symbols.
        if plan.life > 0 {
            apply::apply(
                &mut self.state,
                Cause::CostPayment(ObjectId(0)),
                Event::LifeChanged {
                    player: who,
                    delta: -(plan.life as i32),
                },
                &mut self.log,
            );
        }
    }

    // ---- resolution -----------------------------------------------------

    // ---- announcement (CR 601.2) -----------------------------------------

    /// Put a spell or ability on the stack and begin its announcement.
    ///
    /// Returns `true` when announcement needs targets, in which case the caller sets
    /// `Phase::Announcing`.
    fn begin_announcement(
        &mut self,
        cards: &dyn PrintedCards,
        object: ObjectId,
        controller: PlayerId,
        pay_cost: bool,
        snapshot: Box<GameState>,
        log_len: usize,
    ) -> bool {
        let specs = crate::targeting::specs_of(&self.state, cards, object);
        let modal = crate::targeting::modes_of(&self.state, cards, object);
        let max_x = self.x_bound(cards, object, controller);
        let kicker = pay_cost && self.kicker_affordable(cards, object, controller);
        let cost_parts = self.chosen_cost_parts(cards, object, controller, pay_cost);
        if specs.is_empty()
            && modal.is_none()
            && max_x.is_none()
            && !kicker
            && cost_parts.is_empty()
        {
            let done = Announced::default();
            if !self.complete_announcement(cards, object, controller, pay_cost, &done) {
                self.state = *snapshot;
                self.log.truncate(log_len);
                self.cache.invalidate();
            }
            return false;
        }
        self.announcing = Some(Announcing {
            object,
            controller,
            specs,
            slot: 0,
            chosen: Vec::new(),
            empty: Vec::new(),
            pay_cost,
            snapshot,
            log_len,
            modal,
            modes: None,
            max_x,
            x: None,
            kicker,
            kicked: None,
            cost_parts,
            cost_chosen: Vec::new(),
        });
        true
    }

    /// The parts of an object's cost that are a choice, for announcement: a spell's
    /// additional casting cost, or an activated ability's cost. A triggered ability has none.
    fn chosen_cost_parts(
        &self,
        cards: &dyn PrintedCards,
        object: ObjectId,
        who: PlayerId,
        spell: bool,
    ) -> Vec<mtg_ir::AdditionalCost> {
        let Some(obj) = self.state.objects.get(&object) else {
            return Vec::new();
        };
        let cost = if spell {
            let face = cards.face(obj.card, obj.face).filter(|_| !obj.face_down);
            let mut cost = face.and_then(crate::cost::additional_cast_cost);
            // Cast from the graveyard: retrace's land, escape's exiled cards.
            if obj.cast_context.as_ref().and_then(|c| c.cast_from) == Some(Zone::Graveyard)
                && let Some(face) = face
            {
                let extra = crate::cost::cast_from_additional(face, Zone::Graveyard).additional;
                if !extra.is_empty() {
                    cost.get_or_insert_with(mtg_ir::Cost::free)
                        .additional
                        .extend(extra);
                }
            }
            cost
        } else {
            obj.cast_context
                .as_ref()
                .and_then(|c| Some((c.source?, c.ability?)))
                .and_then(|(source, ability)| {
                    crate::cost::ability_cost(&self.state, cards, source, ability)
                })
        };
        let source = if spell {
            object
        } else {
            obj.cast_context
                .as_ref()
                .and_then(|c| c.source)
                .unwrap_or(object)
        };
        cost.map(|c| c.additional)
            .unwrap_or_default()
            .into_iter()
            .filter(|p| crate::cost::cost_candidates(&self.state, cards, source, who, p).is_some())
            .collect()
    }

    /// Whether a spell has a kicker that could be paid on top of its cost.
    fn kicker_affordable(&self, cards: &dyn PrintedCards, object: ObjectId, who: PlayerId) -> bool {
        let Some(obj) = self.state.objects.get(&object) else {
            return false;
        };
        if obj.face_down {
            return false;
        }
        let Some(k) = cards.face(obj.card, obj.face).and_then(crate::cost::kicker) else {
            return false;
        };
        let Some(mut total) = crate::cost::total_cost(&self.state, cards, object, who) else {
            return false;
        };
        total.symbols.extend(k.symbols);
        crate::mana::plan_spell(&self.state, cards, who, &total, 0, object).is_some()
    }

    /// The most `{X}` could be for this object, or `None` when its cost has no `{X}`.
    ///
    /// The largest value whose total cost is still payable: CR 601.2b lets a player
    /// announce any value, but one that cannot be paid only makes the announcement
    /// illegal, so it is not offered.
    fn x_bound(&self, cards: &dyn PrintedCards, object: ObjectId, who: PlayerId) -> Option<u32> {
        let obj = self.state.objects.get(&object)?;
        let cost = match obj
            .cast_context
            .as_ref()
            .and_then(|c| Some((c.source?, c.ability?)))
        {
            Some((source, ability)) => {
                crate::cost::ability_cost(&self.state, cards, source, ability)
                    .map(|c| c.mana)
                    .or_else(|| {
                        // The source may be gone already (a sacrifice cost is paid later, but
                        // the stack object is a copy of it either way).
                        crate::abilities::find(&self.state, cards, object, ability).and_then(|a| {
                            match &a.kind {
                                mtg_ir::AbilityKind::Activated { cost, .. } => {
                                    Some(cost.mana.clone())
                                }
                                _ => None,
                            }
                        })
                    })?
            }
            None => crate::cost::total_cost(&self.state, cards, object, who)?,
        };
        if !cost.symbols.contains(&mtg_core::ManaSymbol::Variable) {
            return None;
        }
        let spell = obj
            .cast_context
            .as_ref()
            .is_none_or(|c| c.ability.is_none());
        let payable = |x| {
            if spell {
                crate::mana::plan_spell(&self.state, cards, who, &cost, x, object).is_some()
            } else {
                crate::mana::can_pay(&self.state, cards, who, &cost, x)
            }
        };
        let mut x = 0;
        while x < 100 && payable(x + 1) {
            x += 1;
        }
        Some(x)
    }

    /// Choose modes, then X, then fill each target slot, then finish the announcement.
    fn drive_announcement(&mut self, cards: &dyn PrintedCards) -> Option<Choice> {
        let mut a = self.announcing.take()?;

        match a.awaiting() {
            Awaiting::Kicker => {
                let buyback = self
                    .state
                    .objects
                    .get(&a.object)
                    .and_then(|o| cards.face(o.card, o.face))
                    .is_some_and(crate::cost::is_buyback);
                let choice = self.new_choice(
                    a.controller,
                    ChoiceKind::Confirm,
                    if buyback {
                        "pay the buyback cost?".into()
                    } else if self
                        .state
                        .objects
                        .get(&a.object)
                        .and_then(|o| cards.face(o.card, o.face))
                        .is_some_and(crate::cost::is_entwine)
                    {
                        "pay the entwine cost and choose every mode?".into()
                    } else {
                        "pay the kicker cost?".into()
                    },
                    Some(Answer::Bool(false)),
                );
                self.announcement_choice = Some(choice.id);
                self.announcing = Some(a);
                return Some(choice);
            }
            Awaiting::Modes => {
                let (labels, count, min) = a.modal.clone()?;
                let choice = self.new_choice(
                    a.controller,
                    ChoiceKind::ChooseModes {
                        available: labels,
                        count,
                        min: (min != count).then_some(min),
                    },
                    "choose modes".into(),
                    Some(Answer::Modes((0..count).collect())),
                );
                self.announcement_choice = Some(choice.id);
                self.announcing = Some(a);
                return Some(choice);
            }
            Awaiting::X => {
                let max = a.max_x.unwrap_or(0);
                let choice = self.new_choice(
                    a.controller,
                    ChoiceKind::ChooseX { min: 0, max },
                    "choose a value for X".into(),
                    Some(Answer::Number(max)),
                );
                self.announcement_choice = Some(choice.id);
                self.announcing = Some(a);
                return Some(choice);
            }
            Awaiting::Targets | Awaiting::Cost => {}
        }

        while a.slot < a.specs.len() {
            let spec = a.specs[a.slot].clone();
            // A slot belonging to a mode that was not chosen stays empty (CR 700.2b).
            if let Some(m) = spec.mode
                && !a.modes.as_ref().is_some_and(|ms| ms.contains(&m))
            {
                a.skip_slot();
                continue;
            }
            let legal = crate::targeting::legal_targets(
                &self.state,
                cards,
                &spec,
                a.object,
                a.controller,
                &a.chosen,
            );
            let need =
                crate::targeting::required(&spec, &self.state, cards, a.object, a.controller);

            // A slot that requires more targets than exist makes the announcement
            // illegal, so the whole thing rewinds (CR 601.2).
            if (legal.len() as u32) < need {
                self.state = *a.snapshot;
                self.log.truncate(a.log_len);
                self.cache.invalidate();
                self.announcing = None;
                self.announcement_choice = None;
                return None;
            }

            // An "up to" slot with nothing to point at is left empty.
            if legal.is_empty() {
                a.skip_slot();
                continue;
            }

            // The choice is always emitted, even when only one target is legal.
            // Eliding an obvious choice is *policy* (see `mtg-policy`), not a rule, and
            // duplicating it here would put the same judgement in two places.
            // A deterministic "first legal targets" default. Targeting is mandatory,
            // so there is no "decline" — but any legal choice is a legal answer, and
            // having one lets an unattended client proceed rather than stalling.
            let fallback = Answer::Targets(vec![
                legal.iter().copied().take(need.max(1) as usize).collect(),
            ]);
            let choice = self.new_choice(
                a.controller,
                ChoiceKind::ChooseTargets { slots: vec![legal] },
                "choose targets".into(),
                Some(fallback),
            );
            self.announcement_choice = Some(choice.id);
            self.announcing = Some(a);
            return Some(choice);
        }

        // CR 601.2h — choose what pays the cost parts that are a choice.
        if a.cost_chosen.len() < a.cost_parts.len() {
            let part = a.cost_parts[a.cost_chosen.len()].clone();
            let source = self
                .state
                .objects
                .get(&a.object)
                .and_then(|o| o.cast_context.as_ref())
                .and_then(|c| c.source)
                .unwrap_or(a.object);
            let (mut from, n) =
                crate::cost::cost_candidates(&self.state, cards, source, a.controller, &part)
                    .unwrap_or_default();
            // Something already chosen for an earlier part can't pay again.
            from.retain(|o| !a.cost_chosen.iter().flatten().any(|c| c == o));
            if (from.len() as u32) < n {
                self.state = *a.snapshot;
                self.log.truncate(a.log_len);
                self.cache.invalidate();
                self.announcing = None;
                self.announcement_choice = None;
                return None;
            }
            let crew = matches!(part, mtg_ir::AdditionalCost::TapCreaturesWithPower { .. });
            let because = if crew {
                "tap creatures with enough total power"
            } else if matches!(part, mtg_ir::AdditionalCost::Discard { .. }) {
                "discard to pay the cost"
            } else if matches!(part, mtg_ir::AdditionalCost::ExileFrom { .. }) {
                "exile from your graveyard to pay the cost"
            } else {
                "sacrifice to pay the cost"
            };
            let (min, max, fallback) = if crew {
                // Any set with enough power; the default is the smallest prefix that has it.
                let mut pick = Vec::new();
                for id in &from {
                    if crate::cost::total_power(&self.state, cards, &pick) >= n as i32 {
                        break;
                    }
                    pick.push(*id);
                }
                (1, from.len() as u32, pick)
            } else {
                (n, n, from.iter().copied().take(n as usize).collect())
            };
            let choice = self.new_choice(
                a.controller,
                ChoiceKind::ChooseObjects { from, min, max },
                because.into(),
                Some(Answer::Objects(fallback)),
            );
            self.announcement_choice = Some(choice.id);
            self.announcing = Some(a);
            return Some(choice);
        }

        let (object, controller, pay_cost) = (a.object, a.controller, a.pay_cost);
        let done = Announced {
            targets: a.chosen.clone(),
            empty: a.empty.clone(),
            modes: a.modes.clone().unwrap_or_default(),
            x: a.x.unwrap_or(0),
            kicked: a.kicked.unwrap_or(false),
            paid_with: a
                .cost_parts
                .iter()
                .zip(&a.cost_chosen)
                .filter(|(p, _)| {
                    !matches!(
                        p,
                        mtg_ir::AdditionalCost::TapCreaturesWithPower { .. }
                            | mtg_ir::AdditionalCost::ReturnUnblockedAttacker
                    )
                })
                .flat_map(|(_, c)| c.iter().copied())
                .collect(),
            returned: a
                .cost_parts
                .iter()
                .zip(&a.cost_chosen)
                .filter(|(p, _)| matches!(p, mtg_ir::AdditionalCost::ReturnUnblockedAttacker))
                .flat_map(|(_, c)| c.iter().copied())
                .collect(),
            tapped_for: a
                .cost_parts
                .iter()
                .zip(&a.cost_chosen)
                .filter(|(p, _)| matches!(p, mtg_ir::AdditionalCost::TapCreaturesWithPower { .. }))
                .flat_map(|(_, c)| c.iter().copied())
                .collect(),
        };
        self.announcing = None;
        self.announcement_choice = None;
        if !self.complete_announcement(cards, object, controller, pay_cost, &done) {
            self.state = *a.snapshot;
            self.log.truncate(a.log_len);
            self.cache.invalidate();
        }
        None
    }

    /// Record targets, pay the cost, and finish putting the object on the stack.
    fn complete_announcement(
        &mut self,
        cards: &dyn PrintedCards,
        object: ObjectId,
        controller: PlayerId,
        pay_cost: bool,
        done: &Announced,
    ) -> bool {
        // The mana is planned before anything else is paid, and a cost that can't be paid
        // makes the whole announcement illegal (CR 601.2h): the caller rewinds. Nothing is
        // ever cast or activated for free.
        if pay_cost {
            let total = crate::cost::total_cost(&self.state, cards, object, controller);
            if total
                .as_ref()
                .and_then(|t| {
                    crate::mana::plan_spell(&self.state, cards, controller, t, done.x, object)
                })
                .is_none()
            {
                return false;
            }
        } else if let Some((source, ability)) = self
            .state
            .objects
            .get(&object)
            .and_then(|o| o.cast_context.as_ref())
            .and_then(|c| Some((c.source?, c.ability?)))
            && let Some(cost) = crate::cost::ability_cost(&self.state, cards, source, ability)
            && !cost.mana.symbols.is_empty()
            && !crate::mana::can_pay(&self.state, cards, controller, &cost.mana, done.x)
        {
            return false;
        }
        if let Some(o) = self.state.objects.get_mut(&object) {
            let cc = o.cast_context.get_or_insert_with(Default::default);
            cc.targets = done.targets.clone();
            cc.empty_slots = done.empty.clone();
            cc.modes = done.modes.clone();
            // A triggered ability announces no X: its X slot already holds the
            // triggering event's amount (`detect::event_amount`).
            if done.x != 0 {
                cc.x = done.x;
            }
            cc.kicked = done.kicked;
        }
        // Targets are part of the stack object, and are announced publicly — a trigger
        // watching for "becomes the target of" sees these events. Placeholders for empty
        // slots are not targets.
        let events: Vec<Event> = done
            .targets
            .iter()
            .enumerate()
            .filter(|(i, _)| !done.empty.contains(&(*i as u8)))
            .map(|(_, t)| Event::Targeted { object, target: *t })
            .collect();
        if !events.is_empty() {
            apply::apply_simultaneous(
                &mut self.state,
                Cause::PlayerAction(controller),
                events,
                &mut self.log,
            );
        }

        // What was chosen to pay a cost part: sacrificed from the battlefield, discarded
        // from the hand — each to its owner's graveyard — or exiled from a graveyard.
        for id in &done.paid_with {
            if let Some(o) = self
                .state
                .objects
                .get(id)
                .filter(|o| o.zone.zone == Zone::Graveyard)
            {
                let from = o.zone;
                let new_object = self.state.new_object_id();
                apply::apply(
                    &mut self.state,
                    Cause::CostPayment(object),
                    Event::ZoneChange {
                        object: *id,
                        new_object,
                        from,
                        to: ZoneRef::shared(Zone::Exile),
                        index: None,
                    },
                    &mut self.log,
                );
                continue;
            }
            if self
                .state
                .objects
                .get(id)
                .is_some_and(|o| o.zone.zone == Zone::Hand)
            {
                resolve::discard(
                    &mut self.state,
                    cards,
                    &mut self.log,
                    *id,
                    Cause::CostPayment(object),
                );
                continue;
            }
            if let Some(o) = self.state.objects.get(id) {
                let (from, owner, player) = (o.zone, o.owner, o.controller);
                let new_object = self.state.new_object_id();
                apply::apply(
                    &mut self.state,
                    Cause::CostPayment(object),
                    Event::ZoneChange {
                        object: *id,
                        new_object,
                        from,
                        to: ZoneRef::of(Zone::Graveyard, owner),
                        index: None,
                    },
                    &mut self.log,
                );
                apply::apply(
                    &mut self.state,
                    Cause::CostPayment(object),
                    Event::Sacrificed {
                        player,
                        object: *id,
                    },
                    &mut self.log,
                );
            }
        }
        // Ninjutsu: the returned attacker goes to its owner's hand, and what it was
        // attacking is remembered on the ability for its effect.
        for id in &done.returned {
            let Some(o) = self.state.objects.get(id) else {
                continue;
            };
            let (from, owner) = (o.zone, o.owner);
            if let Some(defender) = self.state.combat.attackers.get(id).copied()
                && let Some(stack) = self.state.objects.get_mut(&object)
            {
                stack
                    .cast_context
                    .get_or_insert_with(Default::default)
                    .bindings
                    .insert(mtg_ir::selector::Binding::NINJUTSU_DEFENDER, vec![defender]);
            }
            let new_object = self.state.new_object_id();
            apply::apply(
                &mut self.state,
                Cause::CostPayment(object),
                Event::ZoneChange {
                    object: *id,
                    new_object,
                    from,
                    to: ZoneRef::of(Zone::Hand, owner),
                    index: None,
                },
                &mut self.log,
            );
        }
        for id in &done.tapped_for {
            apply::apply(
                &mut self.state,
                Cause::CostPayment(object),
                Event::TapChanged {
                    object: *id,
                    tapped: true,
                },
                &mut self.log,
            );
        }
        // A spell's additional casting cost in life (CR 119.4).
        if pay_cost
            && let Some(extra) = self
                .state
                .objects
                .get(&object)
                .filter(|o| !o.face_down)
                .and_then(|o| cards.face(o.card, o.face))
                .and_then(crate::cost::additional_cast_cost)
        {
            for part in &extra.additional {
                if let mtg_ir::AdditionalCost::PayLife {
                    amount: mtg_ir::Value::Fixed(n),
                } = part
                {
                    apply::apply(
                        &mut self.state,
                        Cause::CostPayment(object),
                        Event::LifeChanged {
                            player: controller,
                            delta: -*n,
                        },
                        &mut self.log,
                    );
                }
            }
        }

        if pay_cost {
            let total = crate::cost::total_cost(&self.state, cards, object, controller);
            if let Some(plan) = total.as_ref().and_then(|t| {
                crate::mana::plan_spell(&self.state, cards, controller, t, done.x, object)
            }) {
                let before = self.log.len();
                self.pay(cards, controller, &plan);
                // What was spent, for sunburst, converge and "if {R} was spent".
                let mut mana: Vec<(Option<mtg_core::Color>, u16)> = Vec::new();
                for e in &self.log[before..] {
                    if let Event::ManaSpent {
                        player,
                        color,
                        amount,
                    } = &e.event
                        && *player == controller
                    {
                        match mana.iter_mut().find(|(c, _)| c == color) {
                            Some((_, n)) => *n += amount,
                            None => mana.push((*color, *amount)),
                        }
                    }
                }
                apply::apply(
                    &mut self.state,
                    Cause::PlayerAction(controller),
                    Event::SpentToCast { object, mana },
                    &mut self.log,
                );
            }
            apply::apply(
                &mut self.state,
                Cause::PlayerAction(controller),
                Event::SpellCast { object, controller },
                &mut self.log,
            );
        } else {
            self.pay_ability_cost(cards, object, controller, done.x);
        }

        self.state.consecutive_passes = 0;
        true
    }

    /// Pay an activated ability's cost, once its targets are chosen (CR 602.2b, 601.2g-h).
    ///
    /// A triggered ability on the stack has no cost and passes through untouched. Which
    /// parts can be paid was already checked when the ability was offered
    /// ([`crate::cost::additional_payable`]), so this only carries them out.
    fn pay_ability_cost(
        &mut self,
        cards: &dyn PrintedCards,
        object: ObjectId,
        who: PlayerId,
        x: u32,
    ) {
        use mtg_ir::{AdditionalCost as A, Selector};
        let Some((source, ability)) = self
            .state
            .objects
            .get(&object)
            .and_then(|o| o.cast_context.as_ref())
            .and_then(|c| Some((c.source?, c.ability?)))
        else {
            return;
        };
        let Some(cost) = crate::cost::ability_cost(&self.state, cards, source, ability) else {
            return;
        };
        let loyalty =
            crate::abilities::find(&self.state, cards, source, ability).is_some_and(|a| {
                matches!(
                    a.kind,
                    mtg_ir::AbilityKind::Activated {
                        is_loyalty_ability: true,
                        ..
                    }
                )
            });
        apply::apply(
            &mut self.state,
            Cause::CostPayment(source),
            Event::AbilityActivated {
                source,
                ability,
                loyalty,
            },
            &mut self.log,
        );

        if !cost.mana.symbols.is_empty()
            && let Some(plan) = crate::mana::plan(&self.state, cards, who, &cost.mana, x)
        {
            self.pay(cards, who, &plan);
        }

        let cause = Cause::CostPayment(source);
        for part in &cost.additional {
            match part {
                A::Tap {
                    what: Selector::SelfSource,
                }
                | A::Untap {
                    what: Selector::SelfSource,
                } => {
                    let tapped = matches!(part, A::Tap { .. });
                    apply::apply(
                        &mut self.state,
                        cause,
                        Event::TapChanged {
                            object: source,
                            tapped,
                        },
                        &mut self.log,
                    );
                }
                A::PayLife { amount } => {
                    let n = match amount {
                        mtg_ir::Value::Fixed(n) => *n,
                        _ => 0,
                    };
                    apply::apply(
                        &mut self.state,
                        cause,
                        Event::LifeChanged {
                            player: who,
                            delta: -n,
                        },
                        &mut self.log,
                    );
                }
                A::PayEnergy {
                    amount: mtg_ir::Value::Fixed(n),
                } => {
                    apply::apply(
                        &mut self.state,
                        cause,
                        Event::EnergyChanged {
                            player: who,
                            delta: -n,
                        },
                        &mut self.log,
                    );
                }
                A::Loyalty { delta } => {
                    apply::apply(
                        &mut self.state,
                        cause,
                        Event::CountersChanged {
                            object: source,
                            kind: mtg_core::CounterKind::Loyalty,
                            delta: *delta,
                        },
                        &mut self.log,
                    );
                }
                A::RemoveCounters {
                    what: Selector::SelfSource,
                    kind,
                    amount: mtg_ir::Value::Fixed(n),
                } => {
                    apply::apply(
                        &mut self.state,
                        cause,
                        Event::CountersChanged {
                            object: source,
                            kind: *kind,
                            delta: -*n,
                        },
                        &mut self.log,
                    );
                }
                _ => {}
            }
        }

        // Discarding the source from hand (cycling).
        if cost.additional.iter().any(|c| {
            matches!(
                c,
                A::Discard {
                    filter: mtg_ir::ObjectFilter::IsSelf,
                    ..
                }
            )
        }) && self
            .state
            .objects
            .get(&source)
            .is_some_and(|o| o.zone.zone == Zone::Hand)
        {
            let player = self.state.objects[&source].controller;
            let card = resolve::discard(&mut self.state, cards, &mut self.log, source, cause);
            // Discarding it for a cycling ability is cycling it (CR 702.29a).
            if let Some(object) = card
                && cost.additional.contains(&A::Cycling)
            {
                apply::apply(
                    &mut self.state,
                    cause,
                    Event::Cycled { player, object },
                    &mut self.log,
                );
            }
        }

        // "Exile this card from your graveyard".
        if cost.additional.iter().any(|c| {
            matches!(
                c,
                A::ExileFrom {
                    zone: Zone::Graveyard,
                    filter: mtg_ir::ObjectFilter::IsSelf,
                    ..
                }
            )
        }) && let Some(from) = self
            .state
            .objects
            .get(&source)
            .map(|o| o.zone)
            .filter(|z| z.zone == Zone::Graveyard)
        {
            let new_object = self.state.new_object_id();
            apply::apply(
                &mut self.state,
                cause,
                Event::ZoneChange {
                    object: source,
                    new_object,
                    from,
                    to: ZoneRef::shared(Zone::Exile),
                    index: None,
                },
                &mut self.log,
            );
        }
        // Sacrificing the source is done last, so the other parts could still see it.
        if cost.additional.iter().any(|c| {
            matches!(
                c,
                A::Sacrifice {
                    what: Selector::SelfSource,
                    ..
                }
            )
        }) && let Some(from) = self.state.objects.get(&source).map(|o| o.zone)
        {
            let owner = self.state.objects[&source].owner;
            let player = self.state.objects[&source].controller;
            let new_object = self.state.new_object_id();
            apply::apply(
                &mut self.state,
                cause,
                Event::ZoneChange {
                    object: source,
                    new_object,
                    from,
                    to: ZoneRef::of(Zone::Graveyard, owner),
                    index: None,
                },
                &mut self.log,
            );
            apply::apply(
                &mut self.state,
                cause,
                Event::Sacrificed {
                    player,
                    object: source,
                },
                &mut self.log,
            );
        }
    }

    /// Resolve the top of the stack, asking for any decisions it needs.
    ///
    /// Returns a choice when resolution cannot finish without one; the game state is
    /// rolled back to where the resolution started before the choice is handed out, so
    /// nothing half-applied is ever observable. `None` means the resolution completed
    /// (or there was nothing to resolve) and the object has left the stack.
    fn try_resolve_top(&mut self, cards: &dyn PrintedCards) -> Option<Choice> {
        let stack = ZoneRef::shared(Zone::Stack);
        let top = self.state.objects_in(stack).first().copied()?;
        let mut obj = self.state.objects.get(&top).cloned()?;

        let controller = obj.controller;
        let ctx_ability = obj.cast_context.as_ref().and_then(|c| c.ability);

        // Pick up an in-flight resolution of this same object, or start a fresh one.
        // A stale `Resolving` (for an object no longer on top) is discarded rather than
        // resumed, which is what makes an interrupted resolution safe.
        let (snapshot, log_len, answers) = match self.resolving.take() {
            Some(r) if r.object == top => (r.snapshot, r.log_len, r.answers),
            _ => (Box::new(self.state.clone()), self.log.len(), Vec::new()),
        };

        // CR 608.2b — targets are checked again now. A target that has become illegal
        // is ignored and the spell does as much as it can; if *every* target is
        // illegal the spell does not resolve at all, and none of its effect happens —
        // not even the parts that did not target.
        let chosen: Vec<Target> = obj
            .cast_context
            .as_ref()
            .map(|c| c.targets.clone())
            .unwrap_or_default();
        let specs = crate::targeting::specs_of(&self.state, cards, top);
        let mut check =
            crate::targeting::recheck(&self.state, cards, &specs, &chosen, top, controller);
        // Placeholders for empty slots select nothing, and a spell whose only chosen
        // targets are placeholders had no targets at all, so cannot fizzle (CR 608.2b).
        let empty = obj
            .cast_context
            .as_ref()
            .map(|c| c.empty_slots.clone())
            .unwrap_or_default();
        if !empty.is_empty() {
            for i in &empty {
                if let Some(l) = check.still_legal.get_mut(*i as usize) {
                    *l = false;
                }
            }
            let real: Vec<usize> = (0..check.still_legal.len())
                .filter(|i| !empty.contains(&(*i as u8)))
                .collect();
            check.fizzles = !real.is_empty() && real.iter().all(|i| !check.still_legal[*i]);
        }

        // CR 702.103e — a bestowed Aura spell whose target is gone is a creature spell
        // instead, and resolves.
        if check.fizzles && obj.bestowed() {
            if let Some(cc) = self
                .state
                .objects
                .get_mut(&top)
                .and_then(|o| o.cast_context.as_mut())
            {
                cc.alt_cost = None;
                cc.targets.clear();
            }
            obj = self.state.objects.get(&top).cloned()?;
            check.fizzles = false;
        }
        if check.fizzles {
            apply::apply(
                &mut self.state,
                Cause::Resolution(top),
                Event::Countered { object: top },
                &mut self.log,
            );
            self.resolving = None;
            self.resolution_choice = None;
            self.finish_resolution(cards, top, &obj, ctx_ability, true, Entering::default());
            return None;
        }

        let mut rc = ResolveCtx::new(top, controller);
        // "This creature" in an ability means the permanent, not the ability's stack
        // object (CR 113.7). While the source is still where it was, point at it; once it
        // has left, the stack object — a copy of it taken when the ability was put on the
        // stack — stands in as last-known information (CR 113.7a).
        if let Some(src) = obj.cast_context.as_ref().and_then(|c| c.source)
            && self.state.objects.contains_key(&src)
        {
            rc.source = src;
        }
        if let Some(cc) = &obj.cast_context {
            rc.targets = cc.targets.clone();
            rc.x = cc.x;
            if !cc.modes.is_empty() {
                rc.modes = Some(cc.modes.clone());
            }
            // "It" in a triggered ability means the thing the event happened to, and
            // that was captured when the ability triggered.
            rc.bindings = cc.bindings.clone();
        }
        rc.target_legal = check.still_legal;
        rc.answers = answers.clone();

        // Which effect resolves depends on *what* this stack object is.
        //
        // An ability on the stack resolves that ability, found by id. A **spell**
        // resolves only its `SpellEffect`, and a permanent spell has none — it simply
        // becomes a permanent. Matching "the first ability" for a spell is wrong and
        // silently resolves an unrelated triggered ability's effect on cast.
        let delayed_effect = obj.cast_context.as_ref().and_then(|c| c.effect.clone());
        let effect = delayed_effect.or_else(|| {
            self.state
                .objects
                .get(&top)
                .and_then(|o| cards.face(o.card, o.face))
                .and_then(|face| match ctx_ability {
                    Some(id) => {
                        crate::abilities::find(&self.state, cards, top, id).map(|a| a.kind.clone())
                    }
                    None => face
                        .abilities
                        .iter()
                        .find(|a| matches!(a.kind, mtg_ir::AbilityKind::SpellEffect(_)))
                        .map(|a| a.kind.clone()),
                })
                .and_then(|kind| match &kind {
                    mtg_ir::AbilityKind::SpellEffect(e) => Some(e.clone()),
                    mtg_ir::AbilityKind::Triggered { effect, .. } => Some(effect.clone()),
                    mtg_ir::AbilityKind::Activated { effect, .. } => Some(effect.clone()),
                    _ => None,
                })
        });
        // A permanent spell that may enter as a copy has the copied object chosen as it
        // resolves (CR 707.9), through the same ask-and-replay path as any other choice.
        let effect = effect.or_else(|| {
            (ctx_ability.is_none() && !obj.face_down)
                .then(|| enters_as_copy(cards, &obj))
                .flatten()
        });

        if let Some(eff) = effect {
            match resolve::resolve(&mut self.state, cards, &mut self.log, &eff, &mut rc) {
                Ok(()) => {}

                // A decision is needed. Roll back and ask.
                Err(resolve::ResolveError::Ask { who, kind, because }) => {
                    self.state = *snapshot.clone();
                    self.log.truncate(log_len);
                    self.cache.invalidate();
                    self.resolving = Some(Resolving {
                        object: top,
                        snapshot,
                        log_len,
                        answers,
                    });
                    let c = self.new_choice(who, *kind, because, None);
                    self.resolution_choice = Some(c.id);
                    return Some(c);
                }

                // An unimplemented primitive or a bad evaluation. The partial effect is
                // rolled back rather than left half-applied: a spell that did half of
                // what it says is worse than one that did nothing, and the log shows
                // exactly what was not handled.
                Err(_other) => {
                    self.state = *snapshot.clone();
                    self.log.truncate(log_len);
                    self.cache.invalidate();
                }
            }
        }

        // A permanent spell's enter question (shock lands, unleash, riot) is asked as it
        // resolves, through the same ask-and-replay path.
        let enter = if ctx_ability.is_none() {
            match resolve::enter_choice(&self.state, cards, &mut rc, top) {
                Ok(answer) => answer,
                Err(resolve::ResolveError::Ask { who, kind, because }) => {
                    self.state = *snapshot.clone();
                    self.log.truncate(log_len);
                    self.cache.invalidate();
                    self.resolving = Some(Resolving {
                        object: top,
                        snapshot,
                        log_len,
                        answers,
                    });
                    let c = self.new_choice(who, *kind, because, None);
                    self.resolution_choice = Some(c.id);
                    return Some(c);
                }
                Err(_) => None,
            }
        } else {
            None
        };

        self.resolving = None;
        self.resolution_choice = None;
        let copy = rc
            .bindings
            .get(&COPY_CHOICE)
            .and_then(|t| t.first())
            .and_then(|t| match t {
                Target::Object(o) => self.state.objects.get(o).map(|o| (o.card, o.face)),
                Target::Player(_) => None,
            });
        self.finish_resolution(
            cards,
            top,
            &obj,
            ctx_ability,
            false,
            Entering {
                copy,
                answer: enter,
            },
        );
        None
    }

    /// Move a resolved object off the stack and hand priority back.
    fn finish_resolution(
        &mut self,
        cards: &dyn PrintedCards,
        top: ObjectId,
        obj: &crate::state::GameObject,
        ctx_ability: Option<mtg_core::AbilityId>,
        fizzled: bool,
        entering: Entering,
    ) {
        let Entering {
            copy,
            answer: enter,
        } = entering;
        let stack = ZoneRef::shared(Zone::Stack);

        apply::apply(
            &mut self.state,
            Cause::Resolution(top),
            Event::Resolved { object: top },
            &mut self.log,
        );

        // Where it goes afterwards: a permanent spell to the battlefield, anything
        // else to its owner's graveyard, an ability nowhere at all.
        // A permanent spell that did not resolve — every target illegal — never enters
        // the battlefield; like any spell that fails to resolve, it goes to the graveyard
        // (CR 608.2b).
        let is_permanent = !fizzled
            && crate::layers::compute(&self.state, cards, top).is_some_and(|c| c.is_permanent());
        let is_ability = ctx_ability.is_some()
            && obj
                .cast_context
                .as_ref()
                .is_some_and(|c| c.source.is_some());
        // An ability is gone once it has resolved: it is not a card and goes to no zone.
        // (It used to be moved to exile, where it lingered as a copy of its source's card.)
        if is_ability {
            apply::apply(
                &mut self.state,
                Cause::Resolution(top),
                Event::CeasedToExist { object: top },
                &mut self.log,
            );
            self.state.priority = Some(self.state.active_player);
            self.state.consecutive_passes = 0;
            self.cache.invalidate();
            return;
        }

        let new_object = self.state.new_object_id();
        let adventure = !fizzled
            && !is_ability
            && obj.face == 1
            && cards.layout(obj.card) == mtg_ir::Layout::Adventure;
        let exile = adventure || obj.cast_context.as_ref().is_some_and(|c| c.exile_on_leave);
        // CR 702.27a — a spell whose buyback was paid returns to its owner's hand as it
        // resolves. Countered or fizzled, it goes to the graveyard like any other.
        let bought_back = !fizzled
            && !is_ability
            && obj.cast_context.as_ref().is_some_and(|c| c.kicked)
            && cards
                .face(obj.card, obj.face)
                .is_some_and(crate::cost::is_buyback);
        // CR 702.88a — rebound: cast from hand, it is exiled as it resolves.
        let rebound = !fizzled
            && !is_ability
            && !is_permanent
            && obj.cast_context.as_ref().and_then(|c| c.cast_from) == Some(Zone::Hand)
            && cards.face(obj.card, obj.face).is_some_and(|f| {
                f.abilities.iter().any(|a| {
                    matches!(
                        a.kind,
                        mtg_ir::AbilityKind::Keyword(mtg_core::Keyword::Rebound)
                    )
                })
            });
        let to = if is_permanent {
            ZoneRef::shared(Zone::Battlefield)
        } else if rebound {
            ZoneRef::shared(Zone::Exile)
        } else if bought_back {
            ZoneRef::of(Zone::Hand, obj.owner)
        } else if exile {
            // Flashback: exiled instead of going anywhere else (CR 702.34a).
            ZoneRef::shared(Zone::Exile)
        } else {
            ZoneRef::of(Zone::Graveyard, obj.owner)
        };
        apply::apply(
            &mut self.state,
            Cause::Resolution(top),
            Event::ZoneChange {
                object: top,
                new_object,
                from: stack,
                to,
                index: None,
            },
            &mut self.log,
        );

        // "At the beginning of your next upkeep, you may cast this card from exile without
        // paying its mana cost" — a delayed trigger whose "this" is the exiled card.
        if rebound && self.state.objects.contains_key(&new_object) {
            let id = self.state.next_delayed;
            self.state.next_delayed += 1;
            self.state.delayed.push(crate::state::DelayedTrigger {
                id,
                source: new_object,
                card: obj.card,
                controller: obj.controller,
                on: mtg_ir::EventPattern::StepBegins {
                    step: mtg_core::Step::Upkeep,
                    whose: mtg_ir::Selector::You,
                },
                effect: mtg_ir::Effect::CastWithoutPaying {
                    what: mtg_ir::Selector::SelfSource,
                    from: Zone::Exile,
                    haste: false,
                },
                bindings: Default::default(),
                targets: Vec::new(),
            });
        }

        if adventure && to.zone == Zone::Exile {
            apply::apply(
                &mut self.state,
                Cause::Resolution(top),
                Event::AdventureExiled {
                    object: new_object,
                    player: obj.controller,
                },
                &mut self.log,
            );
        }

        // It enters as the copy, so its own "enters tapped" and enters triggers are the
        // copied object's (CR 707.9).
        if to.zone == Zone::Battlefield
            && let Some((card, face)) = copy
        {
            apply::apply(
                &mut self.state,
                Cause::Resolution(top),
                Event::BecameCopy {
                    object: new_object,
                    card,
                    face,
                },
                &mut self.log,
            );
        }
        if to.zone == Zone::Battlefield {
            // A permanent spell carries what it was cast with onto the battlefield.
            if let (Some(cc), Some(o)) =
                (&obj.cast_context, self.state.objects.get_mut(&new_object))
            {
                o.cast_x = cc.x;
                o.kicked = cc.kicked;
                o.cast_for = cc.alt_cost;
                // A copy of a spell was not cast (CR 707.12).
                if !obj.is_spell_copy {
                    o.cast_from = cc.cast_from.or(Some(Zone::Hand));
                }
            }
            // Dash (CR 702.109a): it gains haste, and returns to its owner's hand at the
            // beginning of the next end step.
            let dashed = obj.cast_context.as_ref().and_then(|c| c.alt_cost)
                == Some(mtg_ir::ability::AltCost::Dash);
            // Warp (CR 702.185a): exiled at the beginning of the next end step, to be cast
            // from exile on a later turn.
            if obj.cast_context.as_ref().and_then(|c| c.alt_cost)
                == Some(mtg_ir::ability::AltCost::Warp)
            {
                let id = self.state.next_delayed;
                self.state.next_delayed += 1;
                self.state.delayed.push(crate::state::DelayedTrigger {
                    id,
                    source: new_object,
                    card: obj.card,
                    controller: obj.controller,
                    on: mtg_ir::EventPattern::StepBegins {
                        step: mtg_core::Step::End,
                        whose: mtg_ir::Selector::EachPlayer,
                    },
                    effect: mtg_ir::Effect::Sequence(vec![
                        mtg_ir::Effect::MoveZone {
                            what: mtg_ir::Selector::SelfSource,
                            to: Zone::Exile,
                            owner_relative_to: None,
                            position: mtg_ir::effect::ZonePosition::Natural,
                            tapped: false,
                            face_down: false,
                            under_control_of: None,
                        },
                        mtg_ir::Effect::GrantCastLater {
                            what: mtg_ir::Selector::Bound(mtg_ir::selector::Binding::It),
                        },
                    ]),
                    bindings: Default::default(),
                    targets: Vec::new(),
                });
            }
            if dashed {
                let id = self.state.next_delayed;
                self.state.next_delayed += 1;
                self.state.delayed.push(crate::state::DelayedTrigger {
                    id,
                    source: new_object,
                    card: obj.card,
                    controller: obj.controller,
                    on: mtg_ir::EventPattern::StepBegins {
                        step: mtg_core::Step::End,
                        whose: mtg_ir::Selector::EachPlayer,
                    },
                    effect: mtg_ir::Effect::MoveZone {
                        what: mtg_ir::Selector::SelfSource,
                        to: Zone::Hand,
                        owner_relative_to: None,
                        position: mtg_ir::effect::ZonePosition::Natural,
                        tapped: false,
                        face_down: false,
                        under_control_of: None,
                    },
                    bindings: Default::default(),
                    targets: Vec::new(),
                });
            }
            resolve::entered(
                &mut self.state,
                cards,
                &mut self.log,
                new_object,
                Cause::Resolution(top),
                enter,
            );
            // Suspend (CR 702.62a): a creature cast as its last time counter was removed
            // has haste.
            if dashed || obj.cast_context.as_ref().is_some_and(|c| c.gains_haste) {
                resolve::grant_haste(
                    &mut self.state,
                    &mut self.log,
                    new_object,
                    Cause::Resolution(top),
                );
            }
        }

        // CR 303.4f — an Aura spell resolving becomes attached to the object it targeted.
        if is_permanent
            && !is_ability
            && let Some(Target::Object(host)) = obj
                .cast_context
                .as_ref()
                .and_then(|c| c.targets.first().copied())
            && (obj.bestowed()
                || cards.face(obj.card, obj.face).is_some_and(|f| {
                    f.abilities
                        .iter()
                        .any(|a| matches!(a.kind, mtg_ir::AbilityKind::Enchant))
                }))
        {
            apply::apply(
                &mut self.state,
                Cause::Resolution(top),
                Event::Attached {
                    object: new_object,
                    to: Some(host),
                },
                &mut self.log,
            );
        }

        // An Aura with "enchant player" attaches to the player it targeted.
        if is_permanent
            && !is_ability
            && let Some(Target::Player(player)) = obj
                .cast_context
                .as_ref()
                .and_then(|c| c.targets.first().copied())
            && cards.face(obj.card, obj.face).is_some_and(|f| {
                f.abilities
                    .iter()
                    .any(|a| matches!(a.kind, mtg_ir::AbilityKind::Enchant))
            })
        {
            apply::apply(
                &mut self.state,
                Cause::Resolution(top),
                Event::AttachedToPlayer {
                    object: new_object,
                    player,
                },
                &mut self.log,
            );
        }

        // The resolving player gets priority again (CR 117.3b).
        self.state.priority = Some(self.state.active_player);
        self.state.consecutive_passes = 0;
        self.cache.invalidate();
    }

    // ---- turn-based actions ---------------------------------------------

    /// Start or resume this step's turn-based action.
    ///
    /// Returns a choice when the action cannot finish without one. Returning `None`
    /// means the action is complete (or there was none) and the step may move on.
    fn drive_turn_based_action(&mut self, cards: &dyn PrintedCards) -> Option<Choice> {
        // Resume first: a suspended action outranks starting a new one.
        if let Some(state) = self.suspended.clone() {
            return self.resume(cards, state);
        }

        if self.tba_done {
            return None;
        }
        self.tba_done = true;

        let tba = turn::turn_based_action(self.state.step)?;
        self.begin_turn_based_action(cards, tba)
    }

    fn begin_turn_based_action(
        &mut self,
        cards: &dyn PrintedCards,
        tba: turn::TurnBasedAction,
    ) -> Option<Choice> {
        use turn::TurnBasedAction as T;
        match tba {
            T::DeclareAttackers => self.begin_declare_attackers(cards),
            T::DeclareBlockers => self.begin_declare_blockers(cards),
            T::CombatDamage { first_strike } => self.begin_combat_damage(cards, first_strike),
            T::Cleanup => self.begin_cleanup(cards),
            T::UntapAll => self.begin_untap(cards),
            other => {
                self.perform_turn_based_action(cards, other);
                None
            }
        }
    }

    fn resume(&mut self, cards: &dyn PrintedCards, state: Suspended) -> Option<Choice> {
        match state {
            // These two are waiting purely on the player; nothing to advance until
            // the answer arrives, and `advance` re-emits the outstanding choice.
            Suspended::Attackers
            | Suspended::Blockers
            | Suspended::Discarding
            | Suspended::Untapping => None,
            Suspended::OrderingBlockers { queue } => self.next_blocker_ordering(cards, queue),
            Suspended::AssigningDamage {
                queue,
                first_strike,
                settled,
            } => self.next_damage_assignment(cards, queue, first_strike, settled),
        }
    }

    // ---- cleanup (CR 514) ---------------------------------------------------

    /// The largest hand a player may end their turn with (CR 402.2).
    const MAX_HAND_SIZE: usize = 7;

    /// CR 514.1: discard down to hand size first, which may need the player to choose; then
    /// the rest of cleanup.
    /// CR 502.3: untap — first asking, if the active player controls tapped permanents they
    /// may choose not to untap, which of those stay tapped.
    fn begin_untap(&mut self, cards: &dyn PrintedCards) -> Option<Choice> {
        let who = self.state.active_player;
        let optional: Vec<ObjectId> = self
            .state
            .battlefield()
            .into_iter()
            .filter(|id| {
                self.state.objects.get(id).is_some_and(|o| o.tapped)
                    && crate::layers::controller(&self.state, *id) == Some(who)
                    && crate::layers::restricted(&self.state, cards, *id, |r| {
                        matches!(r, mtg_ir::effect::Restriction::MayChooseNotToUntap)
                    })
            })
            .collect();
        if optional.is_empty() {
            self.perform_turn_based_action(cards, turn::TurnBasedAction::UntapAll);
            return None;
        }
        self.suspended = Some(Suspended::Untapping);
        let n = optional.len() as u32;
        Some(self.new_choice(
            who,
            ChoiceKind::ChooseObjects {
                from: optional,
                min: 0,
                max: n,
            },
            "choose permanents to leave tapped".into(),
            Some(Answer::Objects(Vec::new())),
        ))
    }

    fn begin_cleanup(&mut self, cards: &dyn PrintedCards) -> Option<Choice> {
        let who = self.state.active_player;
        let hand = self.state.objects_in(ZoneRef::of(Zone::Hand, who));
        // "You have no maximum hand size" from a permanent they control.
        let unlimited = crate::layers::effects(&self.state, cards).iter().any(|e| {
            matches!(
                e.modification,
                mtg_ir::effect::Modification::Restriction(
                    mtg_ir::effect::Restriction::NoMaximumHandSize
                )
            ) && crate::layers::controller(&self.state, e.source) == Some(who)
        });
        let excess = if unlimited {
            0
        } else {
            hand.len().saturating_sub(Self::MAX_HAND_SIZE)
        };
        if excess == 0 {
            self.perform_turn_based_action(cards, turn::TurnBasedAction::Cleanup);
            return None;
        }
        self.suspended = Some(Suspended::Discarding);
        let default: Vec<ObjectId> = hand.iter().take(excess).copied().collect();
        let plural = if excess == 1 { "card" } else { "cards" };
        Some(self.new_choice(
            who,
            ChoiceKind::ChooseObjects {
                from: hand,
                min: excess as u32,
                max: excess as u32,
            },
            format!("discard {excess} {plural}: hand size is seven").into(),
            Some(Answer::Objects(default)),
        ))
    }

    fn discarded_to_hand_size(
        &mut self,
        cards: &dyn PrintedCards,
        who: PlayerId,
        chosen: &[ObjectId],
    ) {
        let _ = who;
        for o in chosen {
            resolve::discard(
                &mut self.state,
                cards,
                &mut self.log,
                *o,
                Cause::TurnStructure,
            );
        }
        self.suspended = None;
        self.perform_turn_based_action(cards, turn::TurnBasedAction::Cleanup);
    }

    // ---- declare attackers (CR 508) --------------------------------------

    fn begin_declare_attackers(&mut self, cards: &dyn PrintedCards) -> Option<Choice> {
        let who = self.state.active_player;
        let eligible = crate::combat::eligible_attackers(&self.state, cards, who);
        // Nothing can attack: the action completes with no decision to make.
        if eligible.is_empty() {
            return None;
        }
        self.suspended = Some(Suspended::Attackers);
        Some(self.new_choice(
            who,
            ChoiceKind::DeclareAttackers {
                eligible,
                defenders: crate::combat::attack_destinations(&self.state, cards),
            },
            "declare attackers".into(),
            // Attacking with only the creatures that must is always legal, so there is a
            // safe default and a client may take it without asking.
            Some(Answer::Objects(crate::combat::must_attack(
                &self.state,
                cards,
                who,
            ))),
        ))
    }

    /// Record an attack declaration (CR 508.1).
    fn declared_attackers(
        &mut self,
        cards: &dyn PrintedCards,
        attackers: &[(ObjectId, Target)],
    ) -> bool {
        let who = self.state.active_player;
        let eligible = crate::combat::eligible_attackers(&self.state, cards, who);
        let destinations = crate::combat::attack_destinations(&self.state, cards);
        let mut seen = std::collections::BTreeSet::new();
        if attackers
            .iter()
            .any(|(a, d)| !eligible.contains(a) || !destinations.contains(d) || !seen.insert(*a))
            || crate::combat::must_attack(&self.state, cards, who)
                .iter()
                .any(|a| !seen.contains(a))
            || crate::combat::attacking_alone_illegal(
                &self.state,
                cards,
                &attackers.iter().map(|(a, _)| *a).collect::<Vec<_>>(),
            )
        {
            return false;
        }

        let defending_player = destinations.first().and_then(|d| match d {
            Target::Player(p) => Some(*p),
            _ => None,
        });

        // Declaring attackers and tapping them are one turn-based action, so they
        // share a timestamp and no trigger sees a half-declared combat.
        let mut events = Vec::new();
        for (a, defender) in attackers {
            events.push(Event::Attacked {
                attacker: *a,
                defender: *defender,
                defending_player,
            });
            // Vigilance means attacking does not cause tapping (CR 702.21b).
            if !self.has_vigilance(cards, *a) {
                events.push(Event::TapChanged {
                    object: *a,
                    tapped: true,
                });
            }
        }
        apply::apply_simultaneous(&mut self.state, Cause::TurnStructure, events, &mut self.log);
        self.cache.invalidate();
        self.suspended = None;
        true
    }

    /// The total of an object's toxic values (CR 702.164b: several instances add up).
    fn toxic(&self, cards: &dyn PrintedCards, id: ObjectId) -> u32 {
        let Some(obj) = self.state.objects.get(&id) else {
            return 0;
        };
        let Some(ch) = crate::layers::compute(&self.state, cards, id) else {
            return 0;
        };
        let printed: u32 = cards
            .face(obj.card, obj.face)
            .map(|f| {
                f.abilities
                    .iter()
                    .filter(|a| ch.abilities.contains(&a.id))
                    .map(|a| match a.kind {
                        mtg_ir::AbilityKind::Keyword(mtg_core::Keyword::Toxic(n)) => u32::from(n),
                        _ => 0,
                    })
                    .sum()
            })
            .unwrap_or(0);
        let granted: u32 = ch
            .granted_keywords
            .iter()
            .map(|k| match k {
                mtg_core::Keyword::Toxic(n) => u32::from(*n),
                _ => 0,
            })
            .sum();
        printed + granted
    }

    fn has_deathtouch(&self, cards: &dyn PrintedCards, id: ObjectId) -> bool {
        self.has_kw(cards, id, mtg_ir::ability::Keyword::Deathtouch)
    }

    fn has_vigilance(&self, cards: &dyn PrintedCards, id: ObjectId) -> bool {
        self.has_kw(cards, id, mtg_ir::ability::Keyword::Vigilance)
    }

    fn has_kw(&self, cards: &dyn PrintedCards, id: ObjectId, kw: mtg_ir::ability::Keyword) -> bool {
        use crate::eval::{ComputedChars, Ctx};
        let chars = ComputedChars(cards);
        let Some(obj) = self.state.objects.get(&id) else {
            return false;
        };
        let ctx = Ctx {
            state: &self.state,
            cards,
            chars: &chars,
            source: id,
            controller: obj.controller,
            targets: &[],
            target_legal: &[],
            x: 0,
            bindings: crate::empty_bindings(),
        };
        ctx.has_keyword(id, kw).unwrap_or(false)
    }

    // ---- declare blockers (CR 509) ---------------------------------------

    fn begin_declare_blockers(&mut self, cards: &dyn PrintedCards) -> Option<Choice> {
        if self.state.combat.attackers.is_empty() {
            return None;
        }
        let defender = self
            .state
            .combat
            .defending_player
            .or_else(|| {
                turn::living(&self.state)
                    .into_iter()
                    .find(|p| *p != self.state.active_player)
            })
            .unwrap_or(self.state.active_player);

        let eligible = crate::combat::eligible_blockers(&self.state, cards, defender);
        if eligible.is_empty() {
            self.mark_unblocked();
            return None;
        }

        self.suspended = Some(Suspended::Blockers);
        Some(self.new_choice(
            defender,
            ChoiceKind::DeclareBlockers { eligible },
            "declare blockers".into(),
            Some(Answer::Blocks(Vec::new())),
        ))
    }

    /// CR 509.3h — once blockers are declared, each attacker no creature blocks "isn't
    /// blocked": recorded so "attacks and isn't blocked" can trigger.
    fn mark_unblocked(&mut self) {
        let events: Vec<Event> = self
            .state
            .combat
            .attackers
            .keys()
            .filter(|a| !self.state.combat.was_blocked.contains(a))
            .map(|a| Event::BecameUnblocked { attacker: *a })
            .collect();
        if !events.is_empty() {
            apply::apply_simultaneous(&mut self.state, Cause::TurnStructure, events, &mut self.log);
            self.cache.invalidate();
        }
    }

    /// Record a block declaration, rejecting an illegal one.
    fn declared_blockers(
        &mut self,
        cards: &dyn PrintedCards,
        defender: PlayerId,
        pairs: &[(ObjectId, ObjectId)],
    ) -> Result<(), crate::combat::BlockError> {
        use std::collections::BTreeMap;

        let mut blocks: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();
        for (blocker, attacker) in pairs {
            blocks.entry(*attacker).or_default().push(*blocker);
        }

        // Checked as a whole, because menace cannot be judged pairwise.
        crate::combat::validate_blocks(&self.state, cards, defender, &blocks)?;

        let mut events = Vec::new();
        for (attacker, blockers) in &blocks {
            for b in blockers {
                events.push(Event::Blocked {
                    blocker: *b,
                    attacker: *attacker,
                });
            }
        }
        apply::apply_simultaneous(&mut self.state, Cause::TurnStructure, events, &mut self.log);
        self.mark_unblocked();
        self.cache.invalidate();

        // Attackers blocked by more than one creature need their blockers ordered.
        let queue: Vec<ObjectId> = self
            .state
            .combat
            .blocks
            .iter()
            .filter(|(_, bs)| bs.len() > 1)
            .map(|(a, _)| *a)
            .collect();
        self.suspended = Some(Suspended::OrderingBlockers { queue });
        Ok(())
    }

    /// CR 509.2 — ask the attacking player to order one attacker's blockers.
    fn next_blocker_ordering(
        &mut self,
        _cards: &dyn PrintedCards,
        mut queue: Vec<ObjectId>,
    ) -> Option<Choice> {
        while let Some(attacker) = queue.first().copied() {
            let blockers = self
                .state
                .combat
                .blocks
                .get(&attacker)
                .cloned()
                .unwrap_or_default();
            // Fewer than two blockers left: nothing to order, so do not ask.
            if blockers.len() < 2 {
                queue.remove(0);
                continue;
            }
            self.suspended = Some(Suspended::OrderingBlockers {
                queue: queue.clone(),
            });
            let who = crate::layers::controller(&self.state, attacker)
                .unwrap_or(self.state.active_player);
            return Some(self.new_choice(
                who,
                ChoiceKind::OrderBlockers {
                    attacker,
                    blockers: blockers.clone(),
                },
                "order blockers for damage assignment".into(),
                Some(Answer::Order((0..blockers.len()).collect())),
            ));
        }
        self.suspended = None;
        None
    }

    // ---- combat damage (CR 510) ------------------------------------------

    fn begin_combat_damage(
        &mut self,
        cards: &dyn PrintedCards,
        first_strike: bool,
    ) -> Option<Choice> {
        // Attackers whose assignment involves a real decision get asked; the rest are
        // assigned canonically. Same principle as trigger ordering: a forced choice
        // is not a choice.
        let queue: Vec<ObjectId> = self
            .state
            .combat
            .attackers
            .keys()
            .copied()
            .filter(|a| crate::combat::deals_damage_now(&self.state, cards, *a, first_strike))
            .filter(|a| !crate::combat::assignment_is_forced(&self.state, cards, *a))
            .collect();

        self.next_damage_assignment(cards, queue, first_strike, Vec::new())
    }

    fn next_damage_assignment(
        &mut self,
        cards: &dyn PrintedCards,
        mut queue: Vec<ObjectId>,
        first_strike: bool,
        settled: Vec<crate::combat::Assignment>,
    ) -> Option<Choice> {
        if let Some(attacker) = queue.first().copied() {
            let blockers = self
                .state
                .combat
                .blocks
                .get(&attacker)
                .cloned()
                .unwrap_or_default();
            let power = crate::layers::compute(&self.state, cards, attacker)
                .and_then(|c| c.power)
                .unwrap_or(0)
                .max(0) as u32;
            let who = crate::layers::controller(&self.state, attacker)
                .unwrap_or(self.state.active_player);

            self.suspended = Some(Suspended::AssigningDamage {
                queue: queue.clone(),
                first_strike,
                settled,
            });
            return Some(
                self.new_choice(
                    who,
                    ChoiceKind::AssignCombatDamage {
                        attacker,
                        among: blockers,
                        total: power,
                    },
                    "assign combat damage".into(),
                    // The canonical assignment is always legal, so declining to fiddle
                    // with it is a safe default.
                    crate::combat::assign_attacker_damage(
                        &self.state,
                        cards,
                        attacker,
                        *self.state.combat.attackers.get(&attacker)?,
                    )
                    .map(|a| {
                        Answer::DamageAssignment(
                            a.to.iter()
                                .filter_map(|(t, n)| match t {
                                    Target::Object(o) => Some((*o, *n)),
                                    Target::Player(_) => None,
                                })
                                .collect(),
                        )
                    }),
                ),
            );
        }

        queue.clear();
        self.deal_combat_damage(cards, first_strike, &settled);
        self.suspended = None;
        None
    }

    fn perform_turn_based_action(&mut self, cards: &dyn PrintedCards, tba: turn::TurnBasedAction) {
        use turn::TurnBasedAction as T;
        let active = self.state.active_player;

        match tba {
            T::UntapAll => {
                let mine: Vec<ObjectId> = self
                    .state
                    .battlefield()
                    .into_iter()
                    .filter(|id| {
                        self.state.objects.get(id).is_some_and(|o| o.tapped)
                            && !self.keep_tapped.contains(id)
                            && crate::layers::controller(&self.state, *id) == Some(active)
                            // "Doesn't untap during its controller's untap step."
                            && !crate::layers::restricted(&self.state, cards, *id, |r| {
                                matches!(
                                    r,
                                    mtg_ir::effect::Restriction::CantUntapDuringUntapStep
                                )
                            })
                    })
                    .collect();
                let events = mine
                    .into_iter()
                    .map(|id| Event::TapChanged {
                        object: id,
                        tapped: false,
                    })
                    .collect();
                apply::apply_simultaneous(
                    &mut self.state,
                    Cause::TurnStructure,
                    events,
                    &mut self.log,
                );
                // "Doesn't untap during its controller's next untap step": that step has
                // now passed for this player's permanents.
                let passed: Vec<ObjectId> = self
                    .state
                    .continuous
                    .iter()
                    .filter(|e| e.duration == mtg_ir::effect::Duration::ThroughNextUntapStep)
                    .filter(|e| match &e.affected {
                        crate::state::AffectedSet::Fixed(ids) => ids.iter().all(|id| {
                            crate::layers::controller(&self.state, *id).is_none_or(|c| c == active)
                        }),
                        crate::state::AffectedSet::Dynamic(_) => true,
                    })
                    .map(|e| e.id)
                    .collect();
                if !passed.is_empty() {
                    apply::apply_simultaneous(
                        &mut self.state,
                        Cause::TurnStructure,
                        passed
                            .into_iter()
                            .map(|effect| Event::ContinuousEffectEnded { effect })
                            .collect(),
                        &mut self.log,
                    );
                }
                // Summoning sickness wears off at the start of its controller's turn —
                // its computed controller, which a control-changing effect may have made
                // someone other than whoever cast it.
                let theirs: Vec<ObjectId> = self
                    .state
                    .objects
                    .keys()
                    .copied()
                    .filter(|id| crate::layers::controller(&self.state, *id) == Some(active))
                    .collect();
                for id in theirs {
                    if let Some(o) = self.state.objects.get_mut(&id) {
                        o.summoning_sick = false;
                    }
                }
                for p in self.state.players.values_mut() {
                    p.lands_played = 0;
                }
                // Per-turn bookkeeping. Last-known information is dropped too: nothing
                // looks further back than the current turn, and it would otherwise
                // grow for the whole game.
                self.state.triggered_this_turn.clear();
                self.state.spells_cast_this_turn = 0;
                self.state.spells_by_player_last_turn =
                    std::mem::take(&mut self.state.spells_by_player);
                self.state.draws_this_turn.clear();
                self.state.died_this_turn.clear();
                // CR 726.3a — as a turn begins, day becomes night if the previous turn's
                // active player cast no spells, and night becomes day if they cast two or
                // more.
                if let Some(day) = self.state.day {
                    let order = &self.state.turn_order;
                    let prev = order
                        .iter()
                        .position(|p| *p == active)
                        .map(|i| order[(i + order.len() - 1) % order.len()]);
                    let cast = prev
                        .and_then(|p| self.state.spells_by_player_last_turn.get(&p))
                        .copied()
                        .unwrap_or(0);
                    let now = if day && cast == 0 {
                        Some(false)
                    } else if !day && cast >= 2 {
                        Some(true)
                    } else {
                        None
                    };
                    if let Some(day) = now {
                        apply::apply(
                            &mut self.state,
                            Cause::TurnStructure,
                            Event::DayNight { day },
                            &mut self.log,
                        );
                        crate::resolve::follow_day_night(
                            &mut self.state,
                            cards,
                            &mut self.log,
                            Cause::TurnStructure,
                        );
                    }
                }
                self.state.damaged_this_turn.clear();
                self.state.lost_life_this_turn.clear();
                self.state.attacked_this_turn.clear();
                self.state.activated_this_turn.clear();
                self.state.loyalty_activated_this_turn.clear();
                self.state.last_known.clear();
            }

            T::AddLore => {
                let events: Vec<Event> =
                    self.state
                        .battlefield()
                        .into_iter()
                        .filter(|id| crate::layers::controller(&self.state, *id) == Some(active))
                        .filter_map(|id| {
                            let o = self.state.objects.get(&id).filter(|o| !o.face_down)?;
                            cards.face(o.card, o.face)?.abilities.iter().find_map(|a| {
                                match a.kind {
                                    mtg_ir::AbilityKind::Saga { lore, .. } => {
                                        Some(Event::CountersChanged {
                                            object: id,
                                            kind: lore,
                                            delta: 1,
                                        })
                                    }
                                    _ => None,
                                }
                            })
                        })
                        .collect();
                if !events.is_empty() {
                    apply::apply_simultaneous(
                        &mut self.state,
                        Cause::TurnStructure,
                        events,
                        &mut self.log,
                    );
                }
            }
            T::DrawForTurn => {
                // CR 103.7a — in a two-player game the starting player skips their
                // first draw step.
                let skip = self.state.turn == 1 && self.state.turn_order.len() == 2;
                if !skip {
                    let mut rc = ResolveCtx::new(ObjectId(0), active);
                    let _ = resolve::resolve(
                        &mut self.state,
                        cards,
                        &mut self.log,
                        &mtg_ir::Effect::Draw {
                            who: mtg_ir::Selector::You,
                            count: mtg_ir::Value::ONE,
                        },
                        &mut rc,
                    );
                }
            }

            // Handled by the suspendable path in `begin_turn_based_action`, since
            // each needs player decisions inside the action.
            T::DeclareAttackers | T::DeclareBlockers | T::CombatDamage { .. } => {}

            T::EndCombat => {
                self.state.combat.clear();
                self.suspended = None;
            }

            T::Cleanup => {
                // CR 514.2 — damage wears off and until-end-of-turn effects end,
                // simultaneously and with no priority.
                for o in self.state.objects.values_mut() {
                    o.damage = 0;
                    o.dealt_deathtouch_damage = false;
                    // Regeneration shields last for the turn (CR 701.15a).
                    o.regeneration_shields = 0;
                }
                let ended: Vec<ObjectId> = self
                    .state
                    .continuous
                    .iter()
                    .filter(|e| e.duration == mtg_ir::effect::Duration::UntilEndOfTurn)
                    .map(|e| e.id)
                    .collect();
                let mut events: Vec<Event> = ended
                    .into_iter()
                    .map(|effect| Event::ContinuousEffectEnded { effect })
                    .collect();
                if self.state.prevent_combat_damage {
                    events.push(Event::CombatDamagePreventionChanged { active: false });
                }
                events.extend(self.state.prevent_damage_to.iter().map(|target| {
                    Event::DamagePreventionChanged {
                        target: *target,
                        active: false,
                    }
                }));
                events.extend(
                    self.state
                        .damage_shields
                        .iter()
                        .map(|s| Event::DamageShieldEnded { shield: s.id }),
                );
                apply::apply_simultaneous(
                    &mut self.state,
                    Cause::TurnStructure,
                    events,
                    &mut self.log,
                );
                self.cache.invalidate();
            }
        }
    }

    /// CR 510 — combat damage, assigned and dealt simultaneously.
    ///
    /// Assignment itself lives in [`crate::combat`]; this applies the result.
    /// `chosen` holds assignments the attacking player made by hand, which override
    /// the canonical one for those attackers.
    fn deal_combat_damage(
        &mut self,
        cards: &dyn PrintedCards,
        first_strike: bool,
        chosen: &[crate::combat::Assignment],
    ) {
        // Prevented damage never happens: no lifelink, counters, commander
        // damage, deathtouch, loyalty loss, or damage triggers (CR 615.6).
        if self.state.prevent_combat_damage {
            return;
        }
        let mut assignments: Vec<crate::combat::Assignment> = Vec::new();

        for (attacker, defender) in self.state.combat.attackers.clone() {
            if !crate::combat::deals_damage_now(&self.state, cards, attacker, first_strike) {
                continue;
            }
            match chosen.iter().find(|a| a.source == attacker) {
                Some(manual) => assignments.push(manual.clone()),
                None => {
                    if let Some(a) = crate::combat::assign_attacker_damage(
                        &self.state,
                        cards,
                        attacker,
                        defender,
                    ) {
                        assignments.push(a);
                    }
                }
            }
        }

        // Blockers hit back at the attacker they are blocking.
        for (attacker, blockers) in self.state.combat.blocks.clone() {
            for blocker in blockers {
                if !crate::combat::deals_damage_now(&self.state, cards, blocker, first_strike) {
                    continue;
                }
                if let Some(a) =
                    crate::combat::assign_blocker_damage(&self.state, cards, blocker, attacker)
                {
                    assignments.push(a);
                }
            }
        }

        // Wither and infect change what damage does (CR 702.80, 702.90), per source.
        let kinds: Vec<(bool, bool)> = assignments
            .iter()
            .map(|a| {
                (
                    self.has_kw(cards, a.source, mtg_ir::ability::Keyword::Wither),
                    self.has_kw(cards, a.source, mtg_ir::ability::Keyword::Infect),
                )
            })
            .collect();
        // Damage that will actually be dealt: to an object still there and still in this
        // fight, and not prevented by protection (CR 702.16e).
        let still_hit = |source: ObjectId, target: Target| match target {
            Target::Player(_) => true,
            Target::Object(object) => {
                let still_attacked = self.state.combat.attackers.get(&source)
                    != Some(&Target::Object(object))
                    || (crate::layers::compute(&self.state, cards, object)
                        .is_some_and(|c| c.has_type(mtg_core::CardType::Planeswalker))
                        && crate::layers::controller(&self.state, object)
                            == self.state.combat.defending_player);
                still_attacked
                    && self
                        .state
                        .objects
                        .get(&object)
                        .is_some_and(|o| o.zone.zone == Zone::Battlefield)
                    && !crate::eval::protected_from(&self.state, cards, object, source)
            }
        };
        let mut events: Vec<Event> = Vec::new();
        for (a, (wither, infect)) in assignments.iter().zip(kinds) {
            for (target, amount) in &a.to {
                if *amount == 0 || !still_hit(a.source, *target) {
                    continue;
                }
                let amount = crate::prevention::prevent(
                    &self.state,
                    cards,
                    a.source,
                    *target,
                    *amount,
                    true,
                    &mut events,
                );
                if amount == 0 {
                    continue;
                }
                events.push(match target {
                    Target::Object(o) => resolve::object_damage_event(
                        &self.state,
                        cards,
                        a.source,
                        *o,
                        amount,
                        a.deathtouch,
                        wither || infect,
                    ),
                    Target::Player(p) => Event::DamageDealtToPlayer {
                        source: a.source,
                        player: *p,
                        amount,
                        counters: infect,
                    },
                });
            }
        }

        // CR 702.15b — lifelink: the controller gains the life as part of the same event, so
        // a player saved by it is saved before state-based actions look. Only damage
        // actually dealt counts.
        for a in &assignments {
            let dealt: u32 = events
                .iter()
                .map(|e| match e {
                    Event::DamageMarked { source, amount, .. }
                    | Event::DamageDealtToPlayer { source, amount, .. }
                        if *source == a.source =>
                    {
                        *amount
                    }
                    _ => 0,
                })
                .sum();
            if dealt > 0
                && self.has_kw(cards, a.source, mtg_ir::ability::Keyword::Lifelink)
                && let Some(controller) = crate::layers::controller(&self.state, a.source)
            {
                events.push(Event::LifeChanged {
                    player: controller,
                    delta: dealt as i32,
                });
            }
        }

        // CR 702.164c — toxic: combat damage to a player also gives poison counters, as
        // part of the same damage event.
        let hits: Vec<(ObjectId, PlayerId)> = events
            .iter()
            .filter_map(|e| match e {
                Event::DamageDealtToPlayer {
                    source,
                    player,
                    amount,
                    ..
                } if *amount > 0 => Some((*source, *player)),
                _ => None,
            })
            .collect();
        for (source, player) in hits {
            let toxic = self.toxic(cards, source);
            if toxic > 0 {
                events.push(Event::Poisoned {
                    player,
                    amount: toxic,
                });
            }
        }

        // CR 510.2 — all of it at once.
        apply::apply_simultaneous(&mut self.state, Cause::Combat, events, &mut self.log);
        self.cache.invalidate();
    }

    // ---- step and turn advance ------------------------------------------

    fn advance_step(&mut self, cards: &dyn PrintedCards) {
        let needs_fs = self.state.step == Step::DeclareBlockers
            && crate::combat::needs_first_strike_step(&self.state, cards);

        let mut upcoming = turn::next_step(self.state.step, needs_fs);
        // "Skip your draw step" (CR 500.11): it doesn't happen at all.
        if upcoming == Some(Step::Draw) {
            let active = self.state.active_player;
            let skip = crate::layers::effects(&self.state, cards).iter().any(|e| {
                matches!(
                    e.modification,
                    mtg_ir::effect::Modification::Restriction(
                        mtg_ir::effect::Restriction::SkipDrawStep
                    )
                ) && crate::layers::controller(&self.state, e.source) == Some(active)
            });
            if skip {
                upcoming = turn::next_step(Step::Draw, false);
            }
        }
        match upcoming {
            Some(next) => {
                self.state.step = next;
                self.state.priority = Some(self.state.active_player);
                self.phase = Phase::BeginStep;
            }
            None => {
                // Turn over: an extra turn if one is waiting (CR 500.7), else the next living
                // player after whoever's ordinary turn this was.
                let current = self.state.active_player;
                let next = match self.state.extra_turns.pop() {
                    Some(p) => {
                        self.state.rotation_from.get_or_insert(current);
                        p
                    }
                    None => {
                        let from = self.state.rotation_from.take().unwrap_or(current);
                        turn::next_player(&self.state, from)
                    }
                };
                self.state.active_player = next;
                self.state.turn += 1;
                self.state.step = Step::Untap;
                self.state.priority = Some(next);
                self.phase = Phase::BeginStep;
            }
        }
    }

    // ---- helpers ---------------------------------------------------------

    fn new_choice(
        &mut self,
        who: PlayerId,
        kind: ChoiceKind,
        because: Box<str>,
        default: Option<Answer>,
    ) -> Choice {
        let id = self.next_choice_id;
        self.next_choice_id += 1;
        // Asking anyone else anything ends the undo window for everyone before them:
        // what they are shown may depend on what was done, so it can no longer be taken
        // back without them having seen it.
        if self.undo.last().is_some_and(|c| c.who != who) {
            self.undo.clear();
        }
        let undo = self.can_undo(who);
        let c = Choice {
            id,
            who,
            kind,
            because,
            default,
            undo,
        };
        self.pending = Some(c.clone());
        c
    }

    // ---- mulligans (CR 103.5) ----------------------------------------------

    /// The next opening-hand question: keep or mulligan, or which cards go to the bottom.
    fn pregame_question(&mut self) -> Progress {
        let Some(pg) = self.state.pregame.clone() else {
            return Progress::Continue;
        };
        if let Some((who, n)) = pg.to_bottom {
            let hand = self.state.objects_in(ZoneRef::of(Zone::Hand, who));
            let default: Vec<ObjectId> = hand.iter().take(n as usize).copied().collect();
            let plural = if n == 1 { "card" } else { "cards" };
            let c = self.new_choice(
                who,
                ChoiceKind::ChooseObjects {
                    from: hand,
                    min: n,
                    max: n,
                },
                format!("London mulligan: put {n} {plural} on the bottom of your library").into(),
                Some(Answer::Objects(default)),
            );
            self.pregame_choice = Some(c.id);
            return Progress::NeedsChoice(c);
        }
        let Some(&who) = pg.order.get(pg.next) else {
            self.state.pregame = None;
            return Progress::Continue;
        };
        let taken = pg.taken.get(&who).copied().unwrap_or(0);
        let c = self.new_choice(
            who,
            ChoiceKind::KeepOrMulligan {
                mulligans_taken: taken,
            },
            "your opening hand".into(),
            Some(Answer::Bool(true)),
        );
        self.pregame_choice = Some(c.id);
        Progress::NeedsChoice(c)
    }

    fn pregame_answer(&mut self, choice: Choice, answer: Answer) -> Result<(), Illegal> {
        let Some(mut pg) = self.state.pregame.clone() else {
            self.pregame_choice = None;
            return Ok(());
        };
        let refuse = |engine: &mut Self, choice: Choice| {
            engine.pregame_choice = Some(choice.id);
            engine.pending = Some(choice);
            Err(Illegal::WrongAnswerKind)
        };
        match (&choice.kind, answer) {
            (ChoiceKind::KeepOrMulligan { mulligans_taken }, Answer::Bool(keep)) => {
                let who = pg.order[pg.next];
                // Seven mulligans leaves nothing to keep; the eighth answer is a keep either way.
                if keep || *mulligans_taken >= 7 {
                    if *mulligans_taken > 0 {
                        pg.to_bottom = Some((who, (*mulligans_taken).min(7)));
                    } else {
                        pg.next += 1;
                    }
                } else {
                    *pg.taken.entry(who).or_insert(0) += 1;
                    self.redeal(who);
                }
            }
            (ChoiceKind::ChooseObjects { from, min, .. }, Answer::Objects(chosen)) => {
                let mut distinct = chosen.clone();
                distinct.sort_unstable();
                distinct.dedup();
                let valid = distinct.len() == chosen.len()
                    && chosen.len() == *min as usize
                    && chosen.iter().all(|o| from.contains(o));
                if !valid {
                    return refuse(self, choice);
                }
                if let Some((who, _)) = pg.to_bottom.take() {
                    for o in chosen {
                        self.relocate(o, ZoneRef::of(Zone::Library, who), false);
                    }
                }
                pg.next += 1;
            }
            _ => return refuse(self, choice),
        }
        self.pregame_choice = None;
        self.state.pregame = (pg.next < pg.order.len() || pg.to_bottom.is_some()).then_some(pg);
        self.cache.invalidate();
        Ok(())
    }

    /// Shuffle a hand back into its library and draw a fresh seven.
    fn redeal(&mut self, who: PlayerId) {
        let library = ZoneRef::of(Zone::Library, who);
        for id in self.state.objects_in(ZoneRef::of(Zone::Hand, who)) {
            self.relocate(id, library, false);
        }
        let mut order = self.state.zone_order.remove(&library).unwrap_or_default();
        self.state.rng.shuffle(&mut order);
        self.state.zone_order.insert(library, order.clone());
        for id in order.into_iter().take(7) {
            self.relocate(id, ZoneRef::of(Zone::Hand, who), false);
        }
    }

    /// Move an object between zones without it becoming a new object. Only for arranging
    /// cards before the game begins, where no zone-change event has meaning yet — the same
    /// footing as [`GameState::place`].
    fn relocate(&mut self, id: ObjectId, to: ZoneRef, on_top: bool) {
        let Some(obj) = self.state.objects.get_mut(&id) else {
            return;
        };
        let from = obj.zone;
        obj.zone = to;
        if let Some(list) = self.state.zone_order.get_mut(&from) {
            list.retain(|o| *o != id);
        }
        if to.zone.is_ordered() {
            let list = self.state.zone_order.entry(to).or_default();
            if on_top {
                list.insert(0, id);
            } else {
                list.push(id);
            }
        }
        self.state.bump();
    }

    // ---- undo -------------------------------------------------------------

    /// Whether `who` can take back their most recent priority action right now.
    ///
    /// Undo is a rewind to a checkpoint taken when the action was answered. It is
    /// allowed only while it cannot be unfair or leak anything:
    ///
    /// - **Nobody else has been asked anything since.** Other players learn about the
    ///   game only through the questions they are asked (a peer receives a view with
    ///   each question and nothing in between), so until then the action is invisible
    ///   to them. Any question to another player clears the checkpoints.
    /// - **Nothing hidden or random has happened since.** A draw, a shuffle, a card
    ///   leaving a library or an opponent's hand, or a face-down card turning over
    ///   would show the undoing player something they could then act on differently.
    ///
    /// Within those limits undo is repeatable, back through every action taken since
    /// the player last had to wait on anyone.
    pub fn can_undo(&self, who: PlayerId) -> bool {
        let Some(cp) = self.undo.last() else {
            return false;
        };
        cp.who == who
            && !self.log[cp.log_len.min(self.log.len())..]
                .iter()
                .any(|e| reveals_information(&e.event, who))
    }

    /// Remember the engine as it is, before `choice`'s priority action is taken.
    fn checkpoint(&mut self, choice: &Choice) {
        let quiet = self.announcing.is_none()
            && self.resolving.is_none()
            && self.suspended.is_none()
            && self.to_order.is_empty()
            && self.to_place.is_empty();
        if !quiet {
            // Not a point that can be restored completely, so not one to offer.
            self.undo.clear();
            return;
        }
        if self.undo.len() == UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.undo.push(Checkpoint {
            who: choice.who,
            state: self.state.clone(),
            log_len: self.log.len(),
            phase: self.phase,
            tba_done: self.tba_done,
            question: choice.clone(),
        });
    }

    /// Rewind to the newest checkpoint and ask its question again.
    fn undo_last(&mut self, choice: Choice) -> Result<(), Illegal> {
        if !self.can_undo(choice.who) {
            self.pending = Some(choice);
            return Err(Illegal::CannotUndo);
        }
        let Some(cp) = self.undo.pop() else {
            self.pending = Some(choice);
            return Err(Illegal::CannotUndo);
        };

        self.state = cp.state;
        self.log.truncate(cp.log_len);
        self.phase = cp.phase;
        self.tba_done = cp.tba_done;
        self.to_order.clear();
        self.to_place.clear();
        self.suspended = None;
        self.announcing = None;
        self.announcement_choice = None;
        self.resolving = None;
        self.resolution_choice = None;
        self.cache.invalidate();

        // The same question, under a fresh id: ids only ever increase, so a late answer
        // to the question being abandoned can never land on this one.
        let q = cp.question;
        self.new_choice(q.who, q.kind, q.because, q.default);
        Ok(())
    }

    fn game_over(&self) -> Option<Vec<PlayerId>> {
        let alive = turn::living(&self.state);
        (alive.len() <= 1).then_some(alive)
    }

    fn winners(&self) -> Vec<PlayerId> {
        turn::living(&self.state)
    }

    /// Characteristics of an object, through the memoising cache.
    pub fn characteristics(
        &mut self,
        cards: &dyn PrintedCards,
        id: ObjectId,
    ) -> Option<&mtg_core::Characteristics> {
        self.cache.get(&self.state, cards, id)
    }
}

/// A card in hand's suspend ability, if `ability` is it: its time counters and cost.
/// A card's foretell or plot special action, from the hand: its cost, whether it is exiled
/// face down, what casting it later costs, and whether that is only as a sorcery.
#[allow(clippy::type_complexity)]
fn exile_later_of(
    state: &crate::state::GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    ability: mtg_core::AbilityId,
) -> Option<(mtg_ir::Cost, bool, Option<mtg_core::ManaCost>, bool)> {
    let obj = state
        .objects
        .get(&source)
        .filter(|o| o.zone.zone == Zone::Hand)?;
    cards
        .face(obj.card, obj.face)?
        .abilities
        .iter()
        .find(|a| a.id == ability)
        .and_then(|a| match &a.kind {
            mtg_ir::AbilityKind::ExileToCastLater {
                cost,
                face_down,
                later,
                sorcery,
            } => Some((cost.clone(), *face_down, later.clone(), *sorcery)),
            _ => None,
        })
}

fn suspend_of(
    state: &crate::state::GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    ability: mtg_core::AbilityId,
) -> Option<(u32, mtg_ir::Cost, mtg_core::CounterKind)> {
    let obj = state
        .objects
        .get(&source)
        .filter(|o| o.zone.zone == Zone::Hand)?;
    cards
        .face(obj.card, obj.face)?
        .abilities
        .iter()
        .find(|a| a.id == ability)
        .and_then(|a| match &a.kind {
            mtg_ir::AbilityKind::Suspend {
                time,
                cost,
                counter,
            } => Some((*time, cost.clone(), *counter)),
            _ => None,
        })
}

/// CR 702.37c — what casting a card face down costs.
fn face_down_cost() -> mtg_core::ManaCost {
    mtg_core::ManaCost {
        symbols: vec![mtg_core::ManaSymbol::Generic(3)],
    }
}

/// The morph cost of a face-down permanent's card, if `ability` is its morph ability.
fn morph_of(
    state: &crate::state::GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    ability: mtg_core::AbilityId,
) -> Option<(mtg_ir::Cost, bool)> {
    let obj = state.objects.get(&source).filter(|o| o.face_down)?;
    cards
        .face(obj.card, obj.face)?
        .abilities
        .iter()
        .find(|a| a.id == ability)
        .and_then(|a| match &a.kind {
            mtg_ir::AbilityKind::Morph {
                cost, megamorph, ..
            } => Some((cost.clone(), *megamorph)),
            _ => None,
        })
}

/// A card's morph ability, if it has one.
fn morph_ability(face: &mtg_ir::CardFace) -> Option<&mtg_ir::Ability> {
    face.abilities
        .iter()
        .find(|a| matches!(a.kind, mtg_ir::AbilityKind::Morph { .. }))
}

/// How a permanent spell that has just resolved enters: as a copy of something (CR 707.9),
/// and its answer to an enter question (shock lands, unleash, riot).
#[derive(Default)]
struct Entering {
    copy: Option<(mtg_core::CardId, u8)>,
    answer: Option<Answer>,
}

/// Where the choice of what a permanent enters as a copy of is bound while it resolves.
const COPY_CHOICE: mtg_ir::selector::Binding = mtg_ir::selector::Binding::Named(u16::MAX - 3);

/// For a permanent spell that may enter as a copy (CR 707.9): the step that chooses what.
fn enters_as_copy(
    cards: &dyn PrintedCards,
    obj: &crate::state::GameObject,
) -> Option<mtg_ir::Effect> {
    use mtg_ir::effect::ReplacementKind;
    let face = cards.face(obj.card, obj.face)?;
    face.abilities.iter().find_map(|a| match &a.kind {
        mtg_ir::AbilityKind::ReplacementEffect(r) => match &r.kind {
            ReplacementKind::EntersAsCopy { of, optional } => Some(mtg_ir::Effect::Let {
                slot: COPY_CHOICE,
                what: mtg_ir::Selector::ChosenBy {
                    chooser: Box::new(mtg_ir::Selector::You),
                    zone: Zone::Battlefield,
                    filter: of.clone(),
                    count: mtg_ir::Value::ONE,
                    up_to: *optional,
                },
                body: Box::new(mtg_ir::Effect::Nothing),
            }),
            _ => None,
        },
        _ => None,
    })
}

/// A trigger as a player would name it: its card, and the ability's printed text when the
/// card data carries it.
fn trigger_label(cards: &dyn PrintedCards, t: &PendingTrigger) -> Box<str> {
    let Some(face) = cards.face(t.card, t.face) else {
        return "a triggered ability".into();
    };
    let text = t
        .granted
        .as_deref()
        .or_else(|| face.abilities.iter().find(|a| a.id == t.ability))
        .and_then(|a| a.source_text.as_deref());
    match text {
        Some(text) => format!("{} — {text}", face.name).into(),
        None => face.name.clone(),
    }
}

/// Whether an event shows `who` something they did not know, or depends on chance —
/// either of which makes an undo across it unfair.
fn reveals_information(event: &Event, who: PlayerId) -> bool {
    match event {
        Event::Shuffled { .. }
        | Event::Drew { .. }
        | Event::MillLike { .. }
        | Event::AttemptedDrawFromEmptyLibrary { .. }
        | Event::FaceDownChanged { .. }
        | Event::Lost { .. } => true,
        // A card leaving a library, or anyone else's hand, becomes known.
        Event::ZoneChange { from, .. } => {
            from.zone == Zone::Library || (from.zone == Zone::Hand && from.player != Some(who))
        }
        _ => false,
    }
}
