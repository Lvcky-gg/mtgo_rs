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
    /// most expensive useful spell. Counter opposing spells and answer attackers
    /// with effective instant removal during combat, including on an opponent's turn.
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
        // Answer the opposing stack object with a suitable counter, respecting
        // target restrictions, visible protection, tax payment and blast colors.
        if let Some(action) = self.counter(actions, view) {
            return Answer::Action(action);
        }
        if let Some(action) = self.replacement_win_response(actions, view) {
            return Answer::Action(action);
        }
        if let Some(action) = self.combat_response(actions, view) {
            return Answer::Action(action);
        }
        if let Some(action) = self.engine_response(actions, view) {
            return Answer::Action(action);
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
                if !self.productive_response(view, object, face) {
                    return None;
                }
                let card = view.visible.get(&object)?.card?;
                // Counterspells wait for a spell to counter; a blast for a permanent of its
                // color, or it would be cast for nothing.
                if self.counterspells.contains(&(card, face))
                    && !self.color_gated.contains_key(&(card, face))
                {
                    return None;
                }
                if let Some(color) = self.color_gated.get(&(card, face))
                    && !view.visible.values().any(|o| {
                        o.zone.zone == mtg_core::Zone::Battlefield
                            && o.controller != view.viewer
                            && self.has_color(view, mtg_core::Target::Object(o.id), *color)
                    })
                {
                    return None;
                }
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

impl Bot {
    /// Match a counter to the opposing spell or ability currently on top.
    fn counter(&self, actions: &[Action], view: &PlayerView) -> Option<Action> {
        let top = view.visible.get(view.stack.first()?)?;
        if top.controller == view.viewer
            || self.visibly_uncounterable(view, top.id)
            || self.low_impact_stack_object(view, top.id)
        {
            return None;
        }
        actions
            .iter()
            .filter(|a| {
                a.play().is_some_and(|(object, face, land)| {
                    !land
                        && view
                            .visible
                            .get(&object)
                            .and_then(|o| o.card)
                            .is_some_and(|card| {
                                self.counterspells.contains(&(card, face))
                                    && self
                                        .counter_reliability(view, object, face, top.id)
                                        .is_some()
                                    && self.color_gated.get(&(card, face)).is_none_or(|c| {
                                        self.has_color(view, mtg_core::Target::Object(top.id), *c)
                                    })
                            })
                })
            })
            .min_by_key(|a| {
                let mana_value = a
                    .play()
                    .and_then(|(object, face, _)| {
                        let card = view.visible.get(&object)?.card?;
                        self.stats.get(&(card, face)).map(|s| s.mana_value)
                    })
                    .unwrap_or(u32::MAX);
                let reliability = a
                    .play()
                    .and_then(|(object, face, _)| {
                        self.counter_reliability(view, object, face, top.id)
                    })
                    .unwrap_or(u8::MAX);
                (reliability, mana_value, action_object(a))
            })
            .cloned()
    }
}

fn action_object(action: &Action) -> Option<ObjectId> {
    action.play().map(|(object, _, _)| object)
}
