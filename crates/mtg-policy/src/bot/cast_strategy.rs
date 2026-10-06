//! Spell casting and priority decision logic for the bot.
//!
//! The bot's main phase decision tree: play lands first, then loyalty activations, then expensive spells.
//!
//! ## Priority Decision (Main Phase)
//!
//! On the bot's main phase with an empty stack, [`Bot::priority`] chooses an action:
//!
//! 1. **Combat Prevention**: If under attack and have a Fog-like effect, play it first.
//! 2. **Lands**: Play any available land (mana acceleration, highest priority).
//! 3. **Loyalty Activations**: Activate planeswalker abilities in order:
//!    - Prefer **+loyalty abilities** (building loyalty).
//!    - Then **0-loyalty abilities** (no net change).
//!    - Finally **-loyalty abilities** (spending loyalty; used only as fallback).
//! 4. **Spells**: Cast the most expensive spell in hand (highest mana value).
//! 5. **Pass**: Do nothing if the stack is not empty or if no action is legal.
//!
//! ## Casting Logic
//!
//! When multiple spells are castable, the bot picks by mana value:
//! - Cast a 7-cost bomb before a 2-cost utility spell.
//! - Among spells of the same cost, choose by object ID (stable, deterministic).
//!
//! ## Adventure Cards
//!
//! For adventure cards (can be cast as either creature or adventure):
//! - If both faces are legal, prefer the adventure (instant-speed effect).
//! - Don't count the adventure separately when choosing; rank only the creature face.
//!
//! Example:
//! - Hand: [Island, Lightning Bolt (2MV), Counterspell (3MV), Shock (2MV)]
//! - Turn 1: Play Island (land).
//! - Turn 2: Play Shock (cheapest spell available, mana_value <= 2).
//! - Turn 3: Play Counterspell (highest mana value now available).

use mtg_core::ObjectId;
use mtg_engine::{PlayerView, actions::Action, choice::Answer};

use super::Bot;

impl Bot {
    /// Determine the bot's priority action on its turn.
    ///
    /// On its own turn with an empty stack: a land, a loyalty activation, then the
    /// most expensive spell. Otherwise pass.
    ///
    /// ## Decision Tree
    ///
    /// 1. **Under attack with Fog?** If a declared attack threatens this player/planeswalker,
    ///    and we have a pure combat prevention spell (Fog), play it immediately.
    /// 2. **Not our main phase?** Pass (let the engine handle other phases/stack interactions).
    /// 3. **Play a land** if available (mana acceleration first).
    /// 4. **Activate loyalty** if available (prefer +, then 0, then -).
    /// 5. **Cast most expensive spell** (highest mana value, stable ID ordering).
    /// 6. **Pass** if none of the above.
    ///
    /// This method holds pure prevention spells (like Fog) for declared attacks, avoiding
    /// wasteful plays during opponent's main phase while ensuring the bot can respond to threats.
    pub(super) fn priority(&self, actions: &[Action], view: &PlayerView) -> Answer {
        let is_fog = |action: &Action| {
            action.play().is_some_and(|(object, face, land)| {
                !land
                    && view
                        .visible
                        .get(&object)
                        .and_then(|o| o.card)
                        .is_some_and(|card| self.combat_prevention.contains(&(card, face)))
            })
        };
        // Hold pure prevention spells for an opponent's declared attack. Use
        // only projected public combat information and engine-offered actions.
        let threatened = view.visible.values().any(|o| {
            o.attacking
                && o.controller != view.viewer
                && self.stats(view, o.id).is_some_and(|s| s.power > 0)
                && o.attacking_target.is_some_and(|target| match target {
                    mtg_core::Target::Player(p) => p == view.viewer,
                    mtg_core::Target::Object(id) => view
                        .visible
                        .get(&id)
                        .is_some_and(|d| d.controller == view.viewer),
                })
        });
        if !view.prevent_combat_damage
            && view.stack.is_empty()
            && threatened
            && matches!(
                view.step,
                mtg_core::Step::DeclareBlockers | mtg_core::Step::FirstStrikeCombatDamage
            )
            && let Some(action) = actions.iter().find(|a| is_fog(a))
        {
            return Answer::Action(action.clone());
        }
        let my_main = view.active_player == view.viewer
            && matches!(
                view.step,
                mtg_core::Step::PrecombatMain | mtg_core::Step::PostcombatMain
            )
            && view.stack.is_empty();
        if !my_main {
            return Answer::Pass;
        }
        if let Some(land) = actions
            .iter()
            .find(|a| a.play().is_some_and(|(_, _, land)| land))
        {
            return Answer::Action(land.clone());
        }
        // The engine offers only legal activations and enforces the shared limit.
        // Prefer building loyalty; use a zero or minus ability when no plus is offered.
        let activation = actions
            .iter()
            .filter_map(|a| {
                let Action::ActivateAbility { source, ability } = a else {
                    return None;
                };
                let card = view.visible.get(source)?.card?;
                let info = self
                    .loyalty
                    .get(&(card, view.visible.get(source)?.face, *ability))?;
                Some((info.delta, *source, *ability, a))
            })
            .max_by_key(|(delta, source, ability, _)| {
                (
                    *delta,
                    std::cmp::Reverse(*source),
                    std::cmp::Reverse(*ability),
                )
            });
        if let Some((_, _, _, action)) = activation {
            return Answer::Action(action.clone());
        }

        let best = actions
            .iter()
            .filter_map(|a| {
                let (object, face, land) = a.play()?;
                if land || is_fog(a) {
                    return None;
                }
                let card = view.visible.get(&object)?.card?;
                // Get both uses of an adventurer when its Adventure is legal.
                if face == 0
                    && self.layouts.get(&card) == Some(&mtg_ir::Layout::Adventure)
                    && actions.contains(&Action::CastFace { object, face: 1 })
                {
                    return None;
                }
                Some((self.stats.get(&(card, face))?.mana_value, a))
            })
            // Highest mana value; the lowest object id among equals, so the choice is stable.
            .max_by_key(|(mv, a)| (*mv, std::cmp::Reverse(action_object(a))));
        match best {
            Some((_, action)) => Answer::Action(action.clone()),
            None => Answer::Pass,
        }
    }
}

fn action_object(action: &Action) -> Option<ObjectId> {
    action.play().map(|(object, _, _)| object)
}
