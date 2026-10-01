//! Playing a game over a connection.
//!
//! The piece that joins the rules to the wire. `mtg-engine` knows the rules and nothing about
//! networks; `mtg-net` moves bytes and knows nothing about the rules. Neither should depend on
//! the other, so the loop that drives one across the other lives here.
//!
//! # Who does what
//!
//! One peer is **authoritative**: it owns the [`Engine`] and the full game state. The other
//! sends answers and receives redacted views. That asymmetry is forced by hidden information —
//! a symmetric simulation cannot work when neither client may see the other's library — and it
//! is bounded by `mtg-net`'s commit-reveal rather than by trust.
//!
//! ```text
//!   host                                        guest
//!     │  engine.advance()                         │
//!     │  ── choice is mine ──► answer locally     │
//!     │                                           │
//!     │  ── choice is theirs ─────────────────────►│  Snapshot(view) + Ask(choice)
//!     │                                           │  decide()
//!     │◄──────────────────────────────────────────│  Answer
//!     │  engine.answer(..)                        │
//!     │                                           │
//!     │  ── game over ────────────────────────────►│  GameOver
//! ```
//!
//! # Why a snapshot per question rather than event diffs
//!
//! The architecture calls for streaming events, and the engine's log makes that natural. It is
//! deliberately **not** done yet, because events would need their own redaction pass: a raw
//! `ZoneChange` names the card that moved, so forwarding the log would leak exactly what
//! [`mtg_engine::view::project`] exists to hide. A redacting event filter is a real piece of
//! work with a real leak risk, so until it exists this sends a projected snapshot — which is
//! correct, if larger on the wire, and correctness is the side to err on here.

pub mod game;
pub mod guest;
pub mod host;
pub mod matches;

pub use guest::GuestSession;
pub use host::HostSession;

use mtg_engine::{Choice, PlayerView, choice::Answer};

/// How a session ended.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The game finished normally.
    Over { winners: Vec<mtg_core::PlayerId> },
    /// The peer went away. The event log survives, so this is resumable in principle.
    Disconnected,
}

#[derive(Debug)]
pub enum SessionError {
    Wire(mtg_net::WireError),
    /// The engine rejected an answer. Either a modified client or a version mismatch.
    Illegal(mtg_engine::Illegal),
    /// The peer sent something valid but not expected here.
    Unexpected(String),
    /// The loop ran longer than any real game should.
    Stalled,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::Wire(e) => write!(f, "{e}"),
            SessionError::Illegal(e) => write!(f, "the peer sent an illegal answer: {e:?}"),
            SessionError::Unexpected(m) => write!(f, "unexpected message: {m}"),
            SessionError::Stalled => write!(f, "the game stopped making progress"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<mtg_net::WireError> for SessionError {
    fn from(e: mtg_net::WireError) -> Self {
        SessionError::Wire(e)
    }
}

/// Anything that can answer a question put to a player.
///
/// The seam a UI, a policy, or an AI plugs into — all three are the same shape, which is why
/// the engine was built to hand out [`Choice`] values rather than to call into a UI.
pub trait Decide {
    fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer;
}

impl<F> Decide for F
where
    F: FnMut(&Choice, &PlayerView) -> Answer,
{
    fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
        self(choice, view)
    }
}

/// Answers from a [`mtg_policy::Policy`], falling back to the choice's own default.
///
/// This is what an unattended peer uses. A real client wraps its UI in `Decide` and consults
/// the policy first — the same order, with a human where the fallback is.
#[derive(Default)]
pub struct PolicyDecider {
    pub policy: mtg_policy::Policy,
}

impl Decide for PolicyDecider {
    fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
        if let Some(answer) = self.policy.may_auto_answer(choice, view) {
            return answer;
        }
        // No policy opinion and no human: take the rules' own default, which is always legal
        // where one exists.
        choice.default.clone().unwrap_or(Answer::Pass)
    }
}

/// A hard cap on loop iterations, so a bug or a hostile peer produces an error rather than a
/// hang.
pub(crate) const BUDGET: usize = 2_000_000;
