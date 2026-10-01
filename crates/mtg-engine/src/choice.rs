//! Choice points.
//!
//! Every question the rules require a player to answer is a `Choice`. The engine
//! suspends, emits one, and waits. It has no opinion about how the question is
//! presented or whether a human sees it at all — a client may answer
//! automatically from a policy (see `mtg-policy`), and the engine cannot tell the
//! difference.
//!
//! Keeping *every* decision in one type is what makes the rest of the system
//! tractable: the replay log is a list of events plus a list of answers, an AI is
//! a function from `Choice` to `Answer`, and the headless test client is the same
//! function with a script behind it.

use mtg_core::{ManaSymbol, ObjectId, PlayerId, Target};

use crate::triggers::Conflict;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Choice {
    /// Correlates the answer back to the question. Also the idempotency key: a
    /// resent answer for an already-answered choice is dropped, which is what
    /// makes reconnect-mid-prompt safe.
    pub id: u64,
    pub who: PlayerId,
    pub kind: ChoiceKind,
    /// A short description of the rules reason, shown as the prompt's subtitle so
    /// a player is never asked a question without being told why.
    pub because: Box<str>,
    /// What the engine will do if the player declines to act. Present whenever the
    /// rules define a default, which is what lets a client auto-answer safely.
    pub default: Option<Answer>,
    /// Whether [`Answer::Undo`] would be accepted right now, so a client can offer it
    /// only when it would work.
    #[serde(default)]
    pub undo: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ChoiceKind {
    /// Take an action or pass priority.
    Priority {
        legal: crate::actions::LegalActions,
    },

    /// Order simultaneous triggers (CR 603.3b). Only ever emitted when
    /// [`crate::triggers::plan_ordering`] could not prove the order irrelevant —
    /// `conflicts` is that proof's failure, and the UI renders it as the
    /// explanation of why the question is being asked.
    ///
    /// Answered with [`Answer::Order`], the batch indices in the order they go on the
    /// stack — so the first listed resolves last.
    OrderTriggers {
        /// Indices into the batch, in the order they fired.
        triggers: Vec<usize>,
        conflicts: Vec<Conflict>,
        /// What each trigger is, by index, so a client can show the player something better
        /// than "trigger 2". Public: triggers are announced as they go on the stack.
        #[serde(default)]
        labels: Vec<Box<str>>,
    },

    /// Order several applicable replacement effects (CR 616.1).
    OrderReplacements {
        count: usize,
    },

    ChooseTargets {
        /// One entry per target slot, listing what is currently legal.
        slots: Vec<Vec<Target>>,
    },
    ChooseModes {
        available: Vec<Box<str>>,
        /// How many to choose — at most, when `min` is set ("choose one or both").
        count: u8,
        /// The fewest that may be chosen; `None` means exactly `count`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<u8>,
    },
    ChooseX {
        min: u32,
        max: u32,
    },

    /// A yes/no, for optional triggers and "you may" effects.
    Confirm,

    /// Which objects to pick for an effect that says "choose".
    ChooseObjects {
        from: Vec<ObjectId>,
        min: u32,
        max: u32,
    },

    /// CR 704.5j — the legend rule. Distinct from `ChooseObjects` because the
    /// answer means "this one survives and the rest are put into graveyards",
    /// which is not something a generic object choice can express.
    KeepOneLegend {
        name: Box<str>,
        candidates: Vec<ObjectId>,
    },

    /// How to pay a mana cost when more than one payment is possible.
    PayMana {
        remaining: Vec<ManaSymbol>,
    },

    /// CR 508.1 — which creatures attack, and what each attacks.
    /// [`Answer::Objects`] attacks the defending player; [`Answer::Attackers`]
    /// explicitly assigns each creature to a legal destination.
    DeclareAttackers {
        eligible: Vec<ObjectId>,
        #[serde(default)]
        defenders: Vec<Target>,
    },
    /// CR 509.1 — which creatures block what. Each entry is a potential blocker and
    /// the attackers it could legally block. Answered with [`Answer::Blocks`].
    ///
    /// The candidate list is pairwise, so it cannot express menace on its own; a
    /// declaration is checked in full by [`crate::combat::validate_blocks`], and an
    /// illegal one is rejected rather than silently corrected.
    DeclareBlockers {
        eligible: Vec<(ObjectId, Vec<ObjectId>)>,
    },
    /// Order blockers to assign combat damage among (CR 509.2).
    OrderBlockers {
        attacker: ObjectId,
        blockers: Vec<ObjectId>,
    },
    AssignCombatDamage {
        attacker: ObjectId,
        among: Vec<ObjectId>,
        total: u32,
    },

    /// Discard to hand size in cleanup (CR 514.1).
    DiscardToHandSize {
        count: u32,
    },

    /// Keep or shuffle the opening hand (CR 103.4).
    KeepOrMulligan {
        mulligans_taken: u32,
    },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Answer {
    Action(crate::actions::Action),
    Pass,
    Order(Vec<usize>),
    Targets(Vec<Vec<Target>>),
    Modes(Vec<u8>),
    Number(u32),
    Bool(bool),
    Objects(Vec<ObjectId>),
    /// Attack assignments as (attacker, defending player or planeswalker) pairs.
    Attackers(Vec<(ObjectId, Target)>),
    /// Block assignments as (blocker, attacker) pairs.
    ///
    /// A flat object list cannot express this: blocking is a *pairing*, and one
    /// creature blocking two attackers is a different declaration from two creatures
    /// blocking one.
    Blocks(Vec<(ObjectId, ObjectId)>),
    ManaPayment(Vec<ManaSymbol>),
    /// Damage assigned to blockers. With trample, any unassigned damage goes to
    /// the attack destination after every blocker has been assigned lethal damage.
    DamageAssignment(Vec<(ObjectId, u32)>),
    /// Take back this player's most recent priority action instead of answering.
    ///
    /// Accepted only while nobody else has been asked anything since, and nothing hidden
    /// or random has happened — see `Engine`'s undo notes. Offered as an answer rather
    /// than a separate call so it travels the same path, and the same wire, as any other.
    Undo,
}
