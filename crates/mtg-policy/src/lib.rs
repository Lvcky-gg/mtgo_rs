//! Client-side answering policy — the "intuitive" half of trigger handling.
//!
//! The engine asks a question every time the rules require a decision. Following
//! that literally produces the experience people complain about: a stop at every
//! step, a prompt to order two triggers that cannot interact, a target dialog for a
//! spell with exactly one legal target. None of that is required by the rules —
//! the rules require that the *player* decides, not that the player is interrupted.
//!
//! So: policy lives here, outside the engine, and answers what it safely can.
//! Three rules matter, in rough order of how much they improve play.
//!
//! # 1. Auto-pass
//!
//! If the only legal action is to pass, pass. A player with no castable spell and
//! no activatable ability has no decision, and stopping to confirm that is the
//! single largest source of friction in a digital client. Stops are then *opt-in*
//! and specific — hold priority while I have an instant, stop in my opponent's end
//! step — instead of a default of stopping everywhere that has to be manually
//! skipped.
//!
//! # 2. Elide vacuous choices
//!
//! One legal target for a mandatory target, one legal mode, an `X` with a single
//! possible value: answer it. An undo window covers the case where the player
//! wanted to see the board first.
//!
//! # 3. Never elide anything that reveals information
//!
//! The hard line. An auto-answer is only safe when the player learns nothing by
//! being asked. Anything that would reveal a card, or whose answer could reasonably
//! change after seeing a response, is always shown. [`Policy::may_auto_answer`]
//! is where that judgement lives, and it is deliberately the narrowest part of the
//! system.
//!
//! Because policy is a pure function of a [`Choice`] and a [`PlayerView`], it is
//! also the seam an AI plugs into: a bot is a `Policy` that answers everything.

pub mod bot;

use mtg_engine::{
    choice::{Answer, Choice, ChoiceKind},
    view::PlayerView,
};

/// An answer of the right shape for any question, for when nobody has an opinion.
///
/// The question's own default when it has one — always legal. Otherwise the plainest legal
/// answer: triggers in the order they fired, the first legal target in each slot, the first
/// modes, the smallest `X`, the fewest objects allowed. Some questions have no default because
/// the rules define none (the order of interacting triggers is the player's to choose), and a
/// `Pass` there is the wrong kind of answer: the engine would reject it and ask again forever.
///
/// This is not a decision about what is *good*; it is the floor under a bot's heuristics and a
/// UI's "continue" button, so that neither can stall a game.
pub fn well_formed(choice: &Choice, view: &PlayerView) -> Answer {
    if let Some(default) = &choice.default {
        return default.clone();
    }
    match &choice.kind {
        ChoiceKind::Priority { .. } => Answer::Pass,
        ChoiceKind::OrderTriggers { triggers, .. } => Answer::Order((0..triggers.len()).collect()),
        ChoiceKind::OrderReplacements { count } => Answer::Order((0..*count).collect()),
        ChoiceKind::OrderBlockers { blockers, .. } => Answer::Order((0..blockers.len()).collect()),
        ChoiceKind::ChooseTargets { slots } => Answer::Targets(
            slots
                .iter()
                .map(|s| s.iter().take(1).copied().collect())
                .collect(),
        ),
        // The smallest legal answer: the fewest modes allowed.
        ChoiceKind::ChooseModes { count, min, .. } => {
            Answer::Modes((0..min.unwrap_or(*count)).collect())
        }
        ChoiceKind::ChooseX { min, .. } => Answer::Number(*min),
        ChoiceKind::ChooseObjects { from, min, .. } => {
            Answer::Objects(from.iter().take(*min as usize).copied().collect())
        }
        ChoiceKind::KeepOneLegend { candidates, .. } => {
            Answer::Objects(candidates.iter().take(1).copied().collect())
        }
        ChoiceKind::DiscardToHandSize { count } => {
            let hand = view
                .visible
                .values()
                .filter(|o| {
                    o.zone.zone == mtg_core::Zone::Hand && o.zone.player == Some(view.viewer)
                })
                .map(|o| o.id);
            Answer::Objects(hand.take(*count as usize).collect())
        }
        ChoiceKind::DeclareAttackers { .. } => Answer::Objects(Vec::new()),
        ChoiceKind::DeclareBlockers { .. } => Answer::Blocks(Vec::new()),
        ChoiceKind::AssignCombatDamage { .. } => Answer::DamageAssignment(Vec::new()),
        ChoiceKind::PayMana { remaining } => Answer::ManaPayment(remaining.clone()),
        ChoiceKind::Confirm | ChoiceKind::KeepOrMulligan { .. } => Answer::Bool(true),
    }
}

#[derive(Clone, Debug)]
pub struct Policy {
    pub auto_pass: bool,
    /// Stops the player has explicitly asked for, checked before auto-passing.
    pub stops: Stops,
    pub auto_target_when_single: bool,
    pub auto_order_when_engine_says_safe: bool,
    /// Seconds to let the player take back an auto-answered choice.
    pub undo_window_secs: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            auto_pass: true,
            stops: Stops::default(),
            auto_target_when_single: true,
            auto_order_when_engine_says_safe: true,
            undo_window_secs: 3,
        }
    }
}

/// Points where the player wants to be given priority even with nothing obvious
/// to do. Opt-in stops replace MTGO's model of stopping by default and requiring
/// the player to skip.
#[derive(Clone, Debug, Default)]
pub struct Stops {
    pub my_upkeep: bool,
    pub my_draw: bool,
    pub opponent_end_step: bool,
    pub before_combat_damage: bool,
    /// Stop whenever a spell or ability I control is about to resolve.
    pub my_spell_resolving: bool,
    /// Stop whenever I hold an instant-speed play I could make.
    pub when_i_have_a_response: bool,
}

impl Policy {
    /// Answer a choice without involving the player, or decline to.
    ///
    /// Returning `None` means "show it". The function is written so that adding a
    /// new [`ChoiceKind`] defaults to showing it, rather than silently
    /// auto-answering something nobody considered.
    pub fn may_auto_answer(&self, choice: &Choice, view: &PlayerView) -> Option<Answer> {
        match &choice.kind {
            ChoiceKind::Priority { legal } => {
                if !self.auto_pass || !legal.is_only_passing() {
                    return None;
                }
                if self.stops.wants_stop_here(view) {
                    return None;
                }
                Some(Answer::Pass)
            }

            // Only ever auto-ordered when the engine's footprint analysis proved
            // the order cannot matter — and in that case the engine does not emit
            // this choice at all. Reaching here means it *does* matter.
            ChoiceKind::OrderTriggers { .. } => None,

            ChoiceKind::ChooseTargets { slots } => {
                if !self.auto_target_when_single {
                    return None;
                }
                // Every slot must have exactly one legal option, or the player is
                // making a real decision.
                if slots.iter().all(|s| s.len() == 1) {
                    Some(Answer::Targets(slots.iter().map(|s| vec![s[0]]).collect()))
                } else {
                    None
                }
            }

            // Only when there is nothing to choose: every mode, and exactly that many.
            ChoiceKind::ChooseModes {
                available,
                count,
                min,
            } => (available.len() == *count as usize && min.is_none_or(|m| m == *count))
                .then(|| Answer::Modes((0..*count).collect())),

            ChoiceKind::ChooseX { min, max } => (min == max).then_some(Answer::Number(*min)),

            ChoiceKind::ChooseObjects { from, min, max } => {
                (from.len() as u32 == *min && min == max).then(|| Answer::Objects(from.clone()))
            }

            // Everything below is either a real decision or reveals information.
            // Listed explicitly rather than caught by a wildcard so that a new
            // variant is a compile error here and gets considered.
            ChoiceKind::Confirm
            | ChoiceKind::OrderReplacements { .. }
            | ChoiceKind::PayMana { .. }
            | ChoiceKind::DeclareAttackers { .. }
            | ChoiceKind::DeclareBlockers { .. }
            | ChoiceKind::OrderBlockers { .. }
            | ChoiceKind::AssignCombatDamage { .. }
            | ChoiceKind::DiscardToHandSize { .. }
            | ChoiceKind::KeepOneLegend { .. }
            | ChoiceKind::KeepOrMulligan { .. } => None,
        }
    }
}

impl Stops {
    fn wants_stop_here(&self, view: &PlayerView) -> bool {
        use mtg_core::Step;
        let mine = view.active_player == view.viewer;
        match view.step {
            Step::Upkeep if mine => self.my_upkeep,
            Step::Draw if mine => self.my_draw,
            Step::End if !mine => self.opponent_end_step,
            Step::CombatDamage | Step::FirstStrikeCombatDamage => self.before_combat_damage,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::PlayerId;

    fn a_view() -> PlayerView {
        PlayerView {
            revealed_cards: Vec::new(),
            prevent_combat_damage: false,
            prevent_damage_to: Vec::new(),
            viewer: PlayerId(0),
            turn: 1,
            active_player: PlayerId(0),
            step: mtg_core::Step::PrecombatMain,
            priority: Some(PlayerId(0)),
            players: Default::default(),
            visible: Default::default(),
            stack: Vec::new(),
        }
    }

    fn ask(kind: ChoiceKind, default: Option<Answer>) -> Choice {
        Choice {
            id: 1,
            who: PlayerId(0),
            kind,
            because: "test".into(),
            default,
            undo: false,
        }
    }

    #[test]
    fn a_default_is_used_when_there_is_one() {
        let c = ask(ChoiceKind::Confirm, Some(Answer::Bool(false)));
        assert!(matches!(well_formed(&c, &a_view()), Answer::Bool(false)));
    }

    #[test]
    fn interacting_triggers_with_no_default_get_an_order_not_a_pass() {
        // The case that stalled a game: `Pass` is the wrong kind of answer here.
        let c = ask(
            ChoiceKind::OrderTriggers {
                triggers: vec![0, 1, 2],
                conflicts: Vec::new(),
                labels: Vec::new(),
            },
            None,
        );
        assert!(matches!(well_formed(&c, &a_view()), Answer::Order(o) if o == vec![0, 1, 2]));
    }

    #[test]
    fn choices_without_defaults_take_the_smallest_legal_answer() {
        let modes = ask(
            ChoiceKind::ChooseModes {
                available: vec!["a".into(), "b".into()],
                count: 1,
                min: None,
            },
            None,
        );
        assert!(matches!(well_formed(&modes, &a_view()), Answer::Modes(m) if m == vec![0]));
        let x = ask(ChoiceKind::ChooseX { min: 2, max: 9 }, None);
        assert!(matches!(well_formed(&x, &a_view()), Answer::Number(2)));
    }
}
