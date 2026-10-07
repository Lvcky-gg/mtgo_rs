//! Turn structure (CR 500–514).
//!
//! The turn is a fixed sequence of steps, each of which may carry a *turn-based
//! action* — something that happens automatically as the step begins, before any
//! player gets priority (CR 703). Untapping, drawing for turn, declaring attackers
//! and dealing combat damage are all turn-based actions, not abilities: they use no
//! stack and cannot be responded to, though they can and do cause triggers.
//!
//! Two structural facts drive the design here:
//!
//! - **The untap step and the cleanup step normally grant no priority** (CR 502.4,
//!   CR 514.3). Cleanup is stranger still: if anything happens during it, players
//!   *do* get priority and another cleanup step follows.
//! - **The combat damage step can happen twice**, when a creature with first or
//!   double strike is in combat (CR 510.4). So the step list is not a constant; it
//!   depends on the board.

use mtg_core::{PlayerId, Step};

use crate::state::GameState;

/// The steps of a turn, in order, excluding the conditional first-strike damage
/// step which [`next_step`] inserts when the board calls for it.
pub const STEPS: &[Step] = &[
    Step::Untap,
    Step::Upkeep,
    Step::Draw,
    Step::PrecombatMain,
    Step::BeginCombat,
    Step::DeclareAttackers,
    Step::DeclareBlockers,
    Step::CombatDamage,
    Step::EndCombat,
    Step::PostcombatMain,
    Step::End,
    Step::Cleanup,
];

/// What happens automatically as a step begins.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TurnBasedAction {
    /// CR 502.1 — the active player untaps their permanents. No priority follows
    /// until upkeep, and triggers wait until then too.
    UntapAll,
    /// CR 504.1 — the active player draws. Skipped on the first turn of a
    /// two-player game (CR 103.7a).
    DrawForTurn,
    /// CR 508.1 — the active player declares attackers.
    DeclareAttackers,
    /// CR 509.1 — the defending player declares blockers, then the attacking player
    /// orders them.
    DeclareBlockers,
    /// CR 510.1 — combat damage is assigned and dealt simultaneously.
    CombatDamage { first_strike: bool },
    /// CR 511.3 — creatures are removed from combat.
    EndCombat,
    /// CR 714.3b — the active player puts a lore counter on each Saga they control.
    AddLore,
    /// CR 514.1/2 — discard to hand size, then remove damage and end
    /// until-end-of-turn effects, all as a single turn-based action.
    Cleanup,
}

/// The turn-based action for a step, if it has one.
///
/// Note that `Step::CombatDamage` is **always** the non-first-strike step. When a
/// creature with first or double strike is in combat there are two damage steps
/// (CR 510.4): `FirstStrikeCombatDamage` comes first and handles the strikers, and
/// this one handles everyone else plus double strikers. Treating the second step as a
/// first-strike step makes first strikers deal their damage twice and stops ordinary
/// creatures dealing any.
pub fn turn_based_action(step: Step) -> Option<TurnBasedAction> {
    match step {
        Step::Untap => Some(TurnBasedAction::UntapAll),
        Step::Draw => Some(TurnBasedAction::DrawForTurn),
        Step::DeclareAttackers => Some(TurnBasedAction::DeclareAttackers),
        Step::DeclareBlockers => Some(TurnBasedAction::DeclareBlockers),
        Step::FirstStrikeCombatDamage => Some(TurnBasedAction::CombatDamage { first_strike: true }),
        Step::CombatDamage => Some(TurnBasedAction::CombatDamage {
            first_strike: false,
        }),
        // Combat ends when this step ends, after its priority rounds (CR 511.3).
        Step::EndCombat => None,
        Step::Cleanup => Some(TurnBasedAction::Cleanup),
        Step::PrecombatMain => Some(TurnBasedAction::AddLore),
        _ => None,
    }
}

/// The step that follows `step`, or `None` when the turn is over.
///
/// `needs_first_strike_step` is asked only when leaving declare-blockers, and
/// implements CR 510.4: if any creature in combat has first or double strike, there
/// are two combat damage steps.
pub fn next_step(step: Step, needs_first_strike_step: bool) -> Option<Step> {
    match step {
        Step::DeclareBlockers if needs_first_strike_step => Some(Step::FirstStrikeCombatDamage),
        Step::FirstStrikeCombatDamage => Some(Step::CombatDamage),
        Step::Cleanup => None,
        other => {
            let i = STEPS.iter().position(|s| *s == other)?;
            STEPS.get(i + 1).copied()
        }
    }
}

// The first-strike question lives in `crate::combat`, next to the other combat
// keyword checks, rather than being answered in two places.

/// The next player still in the game, in turn order (CR 800.4).
pub fn next_player(state: &GameState, after: PlayerId) -> PlayerId {
    let n = state.turn_order.len();
    let i = state
        .turn_order
        .iter()
        .position(|p| *p == after)
        .unwrap_or(0);
    // Lost players remain in the seating history, but receive neither priority
    // nor subsequent ordinary turns. Bound the scan for terminal states.
    (1..=n)
        .map(|offset| state.turn_order[(i + offset) % n])
        .find(|p| state.players.get(p).is_some_and(|player| !player.has_lost))
        .unwrap_or(after)
}

/// Players still in the game, in turn order.
pub fn living(state: &GameState) -> Vec<PlayerId> {
    state
        .turn_order
        .iter()
        .copied()
        .filter(|p| !state.player(*p).has_lost)
        .collect()
}
