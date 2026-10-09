//! The trigger pipeline (CR 603) — and the ordering analysis that makes it
//! pleasant to play with.
//!
//! # The pipeline
//!
//! Getting this sequence right is most of what makes a rules engine correct, and
//! it is the part client-side implementations usually flatten:
//!
//! ```text
//!   an action or turn-based action proposes events
//!        │
//!        ▼
//!   replacement & prevention pass (CR 614/615)
//!        │   each applicable effect applies at most once;
//!        │   the affected player orders them when several apply
//!        ▼
//!   events are applied to state and appended to the log
//!        │
//!        ▼
//!   triggered abilities observe the events ──► TriggerQueue (fired, not yet on the stack)
//!        │
//!        ▼
//!   state-based actions (CR 704) ──► repeat until none apply
//!        │   SBAs can themselves cause events, which trigger more abilities,
//!        │   so this is a fixpoint loop, not a single pass
//!        ▼
//!   a player would receive priority (CR 117.5)
//!        │
//!        ▼
//!   queued triggers go on the stack in APNAP order;
//!   each player orders their own ──► ORDERING ANALYSIS
//!        │
//!        ▼
//!   priority
//! ```
//!
//! Two details that are easy to get wrong and are load-bearing here: triggers are
//! *collected* when the event happens but *placed* when a player would next
//! receive priority, and state-based actions run to fixpoint **before** any
//! trigger is placed. A permanent that dies to an SBA and a trigger that killed it
//! therefore both resolve in the right order without special-casing.
//!
//! # Why ordering prompts are the real UX problem
//!
//! CR 603.3b requires that when several of a player's triggers would go on the
//! stack at once, that player chooses the relative order. Taken literally this
//! means a prompt, every time, forever — which is the single most common
//! complaint about playing Magic on a computer. A board with a few permanents
//! that each trigger on the same event turns every combat step into a series of
//! modal dialogs, and in the overwhelming majority of them the order is
//! irrelevant: two creatures each getting a counter, two separate life gains,
//! two scries.
//!
//! The fix is not to skip the rule. It is to *prove* when the choice is vacuous.
//! [`mtg_ir::footprint`] derives what each trigger reads and writes; if no two
//! queued triggers touch overlapping state, every ordering produces an identical
//! result, so the engine picks one deterministically and says nothing. When they
//! do overlap, the player is asked — and told which shared thing makes it matter,
//! so the prompt reads like "these interact because both change X's counters"
//! rather than two lines of identical card names.
//!
//! The asymmetry is deliberate: a false "these commute" is a rules violation, a
//! false "these conflict" is one extra click. The analysis is conservative in
//! that direction everywhere.

use mtg_core::{AbilityId, ObjectId, PlayerId, StampedEvent, Timestamp};
use mtg_ir::footprint::{Footprint, Resource, SelectorResolver};

use crate::state::GameState;

/// An ability that has triggered and is waiting to be put on the stack.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct PendingTrigger {
    /// The object whose ability triggered. May already have left the battlefield,
    /// in which case its characteristics come from last-known information.
    pub source: ObjectId,
    /// The source's card, so two copies of the same card's trigger can be recognised
    /// as interchangeable.
    pub card: mtg_core::CardId,
    /// Face that supplied this ability at trigger time.
    pub face: u8,
    pub ability: AbilityId,
    /// Computed at trigger time, not at placement time: control can change in
    /// between, and CR 603.3a fixes the controller when the ability triggers.
    pub controller: PlayerId,
    /// The event that caused this, kept so the UI can explain the trigger and so
    /// `Binding::EventSubject` can be resolved on resolution.
    pub cause: StampedEvent,
    /// When it fired, used as the deterministic tiebreak for auto-ordering.
    pub fired_at: Timestamp,
    /// Read/write analysis of the ability's effect, filled in at placement time
    /// once targets are known.
    pub footprint: Footprint,
    /// Slots captured from the triggering event, so the effect can say "it".
    ///
    /// Captured at detection time rather than read back on resolution: by the time a
    /// "dies" trigger resolves, the creature it refers to is long gone, and CR 603.3a
    /// fixes these values when the ability triggers.
    pub bindings: std::collections::BTreeMap<mtg_ir::selector::Binding, Vec<mtg_core::Target>>,
    /// A delayed trigger's own effect and id (CR 603.7); `None` for an ability on a card.
    pub delayed: Option<(u32, mtg_ir::Effect)>,
    /// A granted ability's text, captured when it triggered: what granted it may be gone
    /// by the time it is put on the stack (see [`crate::abilities`]).
    pub granted: Option<Box<mtg_ir::Ability>>,
}

/// Triggers that have fired and not yet been placed on the stack.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct TriggerQueue {
    pub pending: Vec<PendingTrigger>,
}

impl TriggerQueue {
    pub fn push(&mut self, t: PendingTrigger) {
        self.pending.push(t);
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Drain the queue into per-player batches in APNAP order (CR 603.3b).
    ///
    /// Each batch is one player's simultaneous triggers, which is the unit the
    /// ordering analysis operates on. Takes the order as a slice rather than the
    /// whole state, so the queue stays independent of `GameState`.
    pub fn drain_apnap(&mut self, apnap: &[PlayerId]) -> Vec<(PlayerId, Vec<PendingTrigger>)> {
        let mut out = Vec::new();
        for p in apnap.iter().copied() {
            let batch: Vec<PendingTrigger> = self
                .pending
                .iter()
                .filter(|t| t.controller == p)
                .cloned()
                .collect();
            if !batch.is_empty() {
                out.push((p, batch));
            }
        }
        self.pending.clear();
        out
    }
}

/// What the engine decided to do about one player's simultaneous triggers.
#[derive(Clone, Debug)]
pub enum Ordering {
    /// Provably order-independent. The engine ordered them itself; no prompt was
    /// shown and none was needed.
    Automatic {
        order: Vec<usize>,
        /// Why it was safe, for the game log's "show your work" view.
        reason: AutoReason,
    },
    /// Order can matter. The player must choose, and these are the interactions
    /// that make it matter.
    MustAsk { conflicts: Vec<Conflict> },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AutoReason {
    /// Only one trigger — nothing to order.
    Single,
    /// Every pair provably commutes.
    AllCommute,
}

/// One reason two triggers cannot be auto-ordered.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Conflict {
    pub a: usize,
    pub b: usize,
    /// The specific shared state. Empty when the reason is that both prompt.
    pub over: Vec<Resource>,
    pub kind: ConflictKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum ConflictKind {
    /// One writes what the other reads: the second sees different state.
    ReadWrite,
    /// Both write the same thing: the result depends on who goes last.
    WriteWrite,
    /// Both ask their controller a question, and the answer to one could
    /// reasonably depend on having seen the other resolve.
    BothPrompt,
    /// One or both could not be analysed (a `Native` ability, an unresolvable
    /// selector). Conservatively treated as conflicting.
    Unanalysable,
}

/// Decide whether a player has to be asked to order their simultaneous triggers.
///
/// This is the whole point of the footprint analysis. Returns [`Ordering::Automatic`]
/// only when every pair provably commutes.
pub fn plan_ordering(batch: &[PendingTrigger]) -> Ordering {
    if batch.len() <= 1 {
        return Ordering::Automatic {
            order: (0..batch.len()).collect(),
            reason: AutoReason::Single,
        };
    }

    let mut conflicts = Vec::new();
    for i in 0..batch.len() {
        for j in (i + 1)..batch.len() {
            let (fa, fb) = (&batch[i].footprint, &batch[j].footprint);
            if fa.commutes_with(fb) || interchangeable(&batch[i], &batch[j]) {
                continue;
            }
            let over = fa.conflicts_with(fb);
            let kind = classify(fa, fb, &over);
            conflicts.push(Conflict {
                a: i,
                b: j,
                over,
                kind,
            });
        }
    }

    if conflicts.is_empty() {
        // Deterministic tiebreak: the order they fired, then source id. Both
        // players' clients compute the same order from the same log, which
        // matters because the choice is not transmitted.
        let mut order: Vec<usize> = (0..batch.len()).collect();
        order.sort_by_key(|&i| (batch[i].fired_at, batch[i].source, batch[i].ability));
        Ordering::Automatic {
            order,
            reason: AutoReason::AllCommute,
        }
    } else {
        Ordering::MustAsk { conflicts }
    }
}

/// Whether two triggers are the same ability doing the same thing, so that swapping them
/// cannot change anything: the same card's same ability, the same controller, bound to
/// the same event, with the same footprint.
///
/// The footprint check is what makes this safe. An effect that refers to its own source
/// ("it deals damage equal to its power") reads that source, and two different sources
/// give two different footprints. Targets, when there are any, are chosen as each ability
/// goes on the stack, so whichever order is used the player can make the same choices.
fn interchangeable(a: &PendingTrigger, b: &PendingTrigger) -> bool {
    a.card == b.card
        && a.face == b.face
        && a.ability == b.ability
        && a.controller == b.controller
        && a.bindings == b.bindings
        && a.footprint == b.footprint
}

fn classify(a: &Footprint, b: &Footprint, over: &[Resource]) -> ConflictKind {
    if over
        .iter()
        .any(|r| matches!(r, Resource::Unanalysable | Resource::ObjectClass))
    {
        return ConflictKind::Unanalysable;
    }
    if a.prompts && b.prompts {
        return ConflictKind::BothPrompt;
    }
    let changes = |f: &Footprint, r: &Resource| {
        f.writes.contains(r) || f.gains.contains(r) || f.losses.contains(r)
    };
    let both_write = over.iter().any(|r| changes(a, r) && changes(b, r));
    if both_write {
        ConflictKind::WriteWrite
    } else {
        ConflictKind::ReadWrite
    }
}

/// Resolves selectors against live game state so footprints can be computed with
/// concrete object ids rather than classes.
pub struct StateResolver<'a> {
    pub state: &'a GameState,
    /// The trigger's source, for `Selector::SelfSource`.
    pub source: ObjectId,
    pub controller: PlayerId,
    /// Targets chosen on announcement, for `Selector::Target`.
    pub targets: &'a [mtg_core::Target],
}

impl SelectorResolver for StateResolver<'_> {
    fn resolve_objects(&self, sel: &mtg_ir::Selector) -> Option<Vec<ObjectId>> {
        use mtg_ir::Selector as S;
        match sel {
            S::SelfSource => Some(vec![self.source]),
            S::Target { index } => match self.targets.get(*index as usize) {
                Some(mtg_core::Target::Object(o)) => Some(vec![*o]),
                Some(mtg_core::Target::Player(_)) => Some(Vec::new()),
                None => None,
            },
            S::You | S::Opponents | S::EachPlayer | S::ActivePlayer => Some(Vec::new()),
            S::Union(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    out.extend(self.resolve_objects(p)?);
                }
                Some(out)
            }
            // `All`/`ChosenBy`/`Bound`/`TopOfLibrary` depend on state that the other
            // trigger might change, or on a choice not yet made. Returning `None`
            // widens the footprint to `ObjectClass`, which conflicts with
            // everything — the conservative answer.
            _ => None,
        }
    }

    fn resolve_players(&self, sel: &mtg_ir::Selector) -> Option<Vec<PlayerId>> {
        use mtg_ir::Selector as S;
        match sel {
            S::You => Some(vec![self.controller]),
            S::ActivePlayer => Some(vec![self.state.active_player]),
            S::Opponents => Some(
                self.state
                    .apnap()
                    .into_iter()
                    .filter(|p| *p != self.controller)
                    .collect(),
            ),
            S::EachPlayer => Some(self.state.apnap()),
            S::Target { index } => match self.targets.get(*index as usize) {
                Some(mtg_core::Target::Player(p)) => Some(vec![*p]),
                Some(mtg_core::Target::Object(_)) => Some(Vec::new()),
                None => None,
            },
            S::SelfSource => Some(Vec::new()),
            S::Union(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    out.extend(self.resolve_players(p)?);
                }
                Some(out)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::{Cause, Event, EventId};
    use std::collections::BTreeSet;

    fn trigger(
        fired: u64,
        reads: &[Resource],
        writes: &[Resource],
        prompts: bool,
    ) -> PendingTrigger {
        PendingTrigger {
            source: ObjectId(fired as u32),
            // A different card per trigger: these stand for different abilities, never for
            // copies of one, which would be interchangeable.
            card: mtg_core::CardId(fired as u32),
            face: 0,
            ability: AbilityId(0),
            controller: PlayerId(0),
            cause: StampedEvent {
                id: EventId(fired),
                at: Timestamp(fired),
                cause: Cause::TurnStructure,
                event: Event::Shuffled {
                    player: PlayerId(0),
                },
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

    #[test]
    fn disjoint_triggers_are_auto_ordered() {
        // Two triggers each touching a different permanent: no prompt.
        let batch = vec![
            trigger(1, &[], &[Resource::Object(ObjectId(10))], false),
            trigger(2, &[], &[Resource::Object(ObjectId(11))], false),
        ];
        match plan_ordering(&batch) {
            Ordering::Automatic { reason, order } => {
                assert_eq!(reason, AutoReason::AllCommute);
                assert_eq!(order, vec![0, 1]);
            }
            other => panic!("expected automatic ordering, got {other:?}"),
        }
    }

    #[test]
    fn write_write_on_the_same_object_must_ask() {
        let batch = vec![
            trigger(1, &[], &[Resource::Object(ObjectId(10))], false),
            trigger(2, &[], &[Resource::Object(ObjectId(10))], false),
        ];
        match plan_ordering(&batch) {
            Ordering::MustAsk { conflicts } => {
                assert_eq!(conflicts.len(), 1);
                assert_eq!(conflicts[0].kind, ConflictKind::WriteWrite);
                assert_eq!(conflicts[0].over, vec![Resource::Object(ObjectId(10))]);
            }
            other => panic!("expected a prompt, got {other:?}"),
        }
    }

    #[test]
    fn one_writes_what_the_other_reads_must_ask() {
        let batch = vec![
            trigger(1, &[], &[Resource::Life(PlayerId(0))], false),
            trigger(
                2,
                &[Resource::Life(PlayerId(0))],
                &[Resource::Object(ObjectId(9))],
                false,
            ),
        ];
        match plan_ordering(&batch) {
            Ordering::MustAsk { conflicts } => {
                assert_eq!(conflicts[0].kind, ConflictKind::ReadWrite);
            }
            other => panic!("expected a prompt, got {other:?}"),
        }
    }

    #[test]
    fn shared_reads_alone_still_auto_order() {
        // Both count the same thing; neither changes it.
        let batch = vec![
            trigger(
                1,
                &[Resource::Life(PlayerId(0))],
                &[Resource::Object(ObjectId(1))],
                false,
            ),
            trigger(
                2,
                &[Resource::Life(PlayerId(0))],
                &[Resource::Object(ObjectId(2))],
                false,
            ),
        ];
        assert!(matches!(plan_ordering(&batch), Ordering::Automatic { .. }));
    }

    #[test]
    fn two_prompting_triggers_must_ask_even_when_disjoint() {
        let batch = vec![
            trigger(1, &[], &[Resource::Object(ObjectId(1))], true),
            trigger(2, &[], &[Resource::Object(ObjectId(2))], true),
        ];
        match plan_ordering(&batch) {
            Ordering::MustAsk { conflicts } => {
                assert_eq!(conflicts[0].kind, ConflictKind::BothPrompt);
            }
            other => panic!("expected a prompt, got {other:?}"),
        }
    }

    #[test]
    fn unanalysable_triggers_always_ask() {
        let batch = vec![trigger(1, &[], &[Resource::Object(ObjectId(1))], false), {
            let mut t = trigger(2, &[], &[], false);
            t.footprint = Footprint::unanalysable();
            t
        }];
        assert!(matches!(plan_ordering(&batch), Ordering::MustAsk { .. }));
    }

    #[test]
    fn single_trigger_never_prompts() {
        let batch = vec![trigger(1, &[], &[Resource::Unanalysable], true)];
        match plan_ordering(&batch) {
            Ordering::Automatic { reason, .. } => assert_eq!(reason, AutoReason::Single),
            other => panic!("expected automatic, got {other:?}"),
        }
    }
}
