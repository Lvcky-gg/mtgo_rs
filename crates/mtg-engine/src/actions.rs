//! Legal actions.
//!
//! The engine computes the complete set of things a player may legally do right
//! now, and hands it over. **The UI contains no rules logic** — it renders this
//! list and nothing else. Nothing is clickable unless it appears here.
//!
//! That boundary is doing real work beyond tidiness:
//!
//! - A rules bug cannot hide in the view layer, because the view layer cannot
//!   form an illegal action.
//! - The auto-pass heuristic is a predicate over this list, not a re-derivation
//!   of the rules. "Can this player do anything meaningful?" is
//!   `legal.is_only_passing()`.
//! - An AI and the headless test client consume exactly what the GUI does, so a
//!   scripted test exercises the same path a human does.

use mtg_core::{AbilityId, Color, ObjectId, PlayerId, Target};

/// Something a player can do with priority.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Action {
    /// Cast a spell from a zone.
    Cast {
        object: ObjectId,
    },
    /// Play a land — a special action, not a spell (CR 305.1).
    PlayLand {
        object: ObjectId,
    },
    /// Cast a selected half, modal double-faced face, or Adventure.
    CastFace {
        object: ObjectId,
        face: u8,
    },
    /// Cast a spell from the hand for an alternative cost (dash, evoke): `ability` is the
    /// card's `AlternativeCost` ability.
    CastAlternative {
        object: ObjectId,
        ability: AbilityId,
    },
    /// CR 702.37c — cast a card with morph face down, as a 2/2 creature spell for {3}.
    CastFaceDown {
        object: ObjectId,
    },
    /// Play the selected land face of a modal double-faced card.
    PlayLandFace {
        object: ObjectId,
        face: u8,
    },
    ActivateAbility {
        source: ObjectId,
        ability: AbilityId,
    },
    /// Activate a mana ability by hand: tapping a land to float mana (CR 605).
    ///
    /// Separate from [`Action::ActivateAbility`] because it uses no stack and does not
    /// pass priority, and because the colour is part of the action: a source that makes
    /// "white or green" is offered once per colour, so choosing is picking an action
    /// rather than answering a follow-up question. `None` for colorless, or a source with
    /// only one output.
    ActivateManaAbility {
        source: ObjectId,
        ability: AbilityId,
        color: Option<Color>,
    },
    /// CR 116 special actions: turning a face-down creature up, suspend, etc.
    SpecialAction {
        source: ObjectId,
        ability: AbilityId,
    },
    Pass,
    Concede,
}

impl Action {
    /// Object, selected face, and whether this is a land play.
    pub fn play(&self) -> Option<(ObjectId, u8, bool)> {
        match *self {
            Self::Cast { object } => Some((object, 0, false)),
            Self::PlayLand { object } => Some((object, 0, true)),
            Self::CastFace { object, face } => Some((object, face, false)),
            Self::PlayLandFace { object, face } => Some((object, face, true)),
            _ => None,
        }
    }
}

#[derive(Clone, Default, Debug, serde::Serialize, serde::Deserialize)]
pub struct LegalActions {
    pub who: Option<PlayerId>,
    pub actions: Vec<Action>,
    /// Mana abilities, held separately.
    ///
    /// They are perfectly legal to activate whenever their controller has priority,
    /// but activating one with nothing to spend the mana on accomplishes nothing —
    /// and if they sat in `actions`, `is_only_passing` would be false for anyone
    /// controlling an untapped land, which would disable auto-pass for the entire
    /// game. They are offered to the UI and excluded from the "is there a decision
    /// here?" question.
    pub mana_abilities: Vec<Action>,
    /// Per-action target legality, precomputed so the UI can grey out a spell with
    /// no legal targets instead of letting it be announced and then rewound.
    pub targets: Vec<(Action, Vec<Vec<Target>>)>,
}

impl LegalActions {
    /// Whether the only thing available is passing.
    ///
    /// The auto-pass rule rests on this. A player holding nothing castable, with
    /// no activatable ability, has no decision to make, and stopping to ask is
    /// pure friction — this is the single biggest difference between a client that
    /// is pleasant to play and one that is not.
    pub fn is_only_passing(&self) -> bool {
        self.actions
            .iter()
            .all(|a| matches!(a, Action::Pass | Action::Concede))
    }

    /// Everything a player could do, mana abilities included.
    pub fn all(&self) -> impl Iterator<Item = &Action> {
        self.actions.iter().chain(&self.mana_abilities)
    }

    /// Actions that cannot accomplish anything: activating a mana ability with no
    /// spell to spend it on, or casting a spell whose only target just became
    /// illegal. Excluded from the auto-pass check so that "nothing meaningful"
    /// means what a player would mean by it.
    pub fn meaningful(&self) -> impl Iterator<Item = &Action> {
        self.actions
            .iter()
            .filter(|a| !matches!(a, Action::Pass | Action::Concede))
    }
}
