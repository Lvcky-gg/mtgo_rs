//! The rules engine.
//!
//! The engine is a **pure state machine**. It performs no I/O, reads no clock,
//! and owns no randomness: the only way to advance it is
//!
//! ```text
//! fn apply(&mut self, cmd: Command) -> Result<Vec<StampedEvent>, Illegal>
//! ```
//!
//! and every non-determinism it needs (shuffles, coin flips) arrives inside the
//! command as an explicit seed. Three things follow from that, and together they
//! are most of why the networking design is as simple as it is:
//!
//! - **Replay is exact.** The event log plus the initial state reproduces any
//!   game bit-for-bit, so a replay file is just the log.
//! - **Reconnect is free.** A peer that dropped asks for events after sequence
//!   *n* and catches up. There is no separate resync path.
//! - **Verification is possible.** After a game, both players reveal their decks
//!   and seeds, and either side can replay the log to confirm the host did not
//!   cheat. See `mtg-net`.
//!
//! The engine also never talks to a UI. It publishes the set of currently legal
//! actions ([`actions::LegalActions`]) and blocks on [`choice::Choice`] values.
//! Deciding which prompts a human should actually see is *policy*, and lives in
//! `mtg-policy` — outside the rules.

pub mod abilities;
pub mod actions;
pub mod apply;
pub mod choice;
pub mod combat;
pub mod cost;
pub mod detect;
pub mod engine;
pub mod eval;
pub mod layers;
pub mod mana;
pub mod prevention;
pub mod resolve;
pub mod sba;
pub mod state;
pub mod targeting;
pub mod triggers;
pub mod turn;
pub mod view;

pub use actions::{Action, LegalActions};
pub use choice::{Answer, Choice, ChoiceKind};
pub use engine::{Engine, Illegal, Progress};
pub use state::{GameObject, GameState, PlayerState};
pub use triggers::{PendingTrigger, TriggerQueue};
pub use view::PlayerView;

/// A shared empty binding map, for evaluation contexts that have no bindings.
pub(crate) fn empty_bindings()
-> &'static std::collections::BTreeMap<mtg_ir::selector::Binding, Vec<mtg_core::Target>> {
    use std::sync::OnceLock;
    static EMPTY: OnceLock<
        std::collections::BTreeMap<mtg_ir::selector::Binding, Vec<mtg_core::Target>>,
    > = OnceLock::new();
    EMPTY.get_or_init(Default::default)
}
